/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use atomic_refcell::AtomicRefCell;
use base64::Engine as _;
use cssparser::Parser;
use dom_struct::dom_struct;
use embedder_traits::Theme;
use html5ever::{LocalName, Prefix, local_name, ns};
use js::context::{JSContext, NoGC};
use js::rust::HandleObject;
use layout_api::SVGElementData;
use servo_url::ServoUrl;
use style::attr::AttrValue;
use style::parser::ParserContext;
use style::stylesheets::Origin;
use style::values::specified::LengthPercentage;
use style_traits::ParsingMode;
use uuid::Uuid;
use xml5ever::serialize::TraversalScope;

use crate::dom::bindings::codegen::Bindings::DocumentBinding::DocumentMethods;
use crate::dom::bindings::codegen::Bindings::NodeBinding::NodeMethods;
use crate::dom::bindings::codegen::Bindings::WindowBinding::WindowMethods;
use crate::dom::bindings::codegen::Bindings::CSSStyleDeclarationBinding::CSSStyleDeclarationMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::root::{DomRoot, LayoutDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::document::Document;
use crate::dom::element::attributes::storage::AttrRef;
use crate::dom::element::{AttributeMutation, Element};
use crate::dom::iterators::ShadowIncluding;
use crate::dom::node::virtualmethods::VirtualMethods;
use crate::dom::node::{
    ChildrenMutation, CloneChildrenFlag, Node, NodeDamage, NodeTraits, UnbindContext,
};
use crate::dom::svg::svggraphicselement::SVGGraphicsElement;

#[dom_struct]
pub(crate) struct SVGSVGElement {

    svggraphicselement: SVGGraphicsElement,
    #[no_trace]
    uuid: Uuid,
    // The XML source of subtree rooted at this SVG element, serialized into
    // a base64 encoded `data:` url. This is cached to avoid recomputation
    // on each layout and must be invalidated when the subtree changes.
    #[no_trace]
    cached_serialized_data_url: AtomicRefCell<Option<Result<ServoUrl, ()>>>,
}

/// Paint-affecting longhands materialized onto the serialized SVG copy (see
/// `inline_resolved_paint_styles`). Kept to painting so the copy cannot alter
/// layout, and inclusive of text and gradient stops found in real logos.
const SERIALIZED_SVG_PAINT_PROPERTIES: &[&str] = &[
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stop-color",
    "stop-opacity",
    "color",
    "opacity",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
];

/// Paint properties whose dark grays are auto-lightened under a dark used
/// color scheme (see `lighten_dark_vector_gray`). Only color-valued vector
/// paints: layout, opacity and typography are never remapped.
fn is_remappable_paint_property(property: &str) -> bool {
    matches!(property, "fill" | "stroke" | "stop-color" | "color")
}

/// Upper sRGB relative luminance considered "dark gray" for icon remapping.
/// Catches true black through roughly `#7d7d7d`; lighter grays already
/// contrast against dark backgrounds.
const DARK_VECTOR_GRAY_LUMINANCE: f32 = 0.21;

/// Maximum channel spread (0-255) still considered gray rather than a brand
/// color. Saturated fills such as Yahoo purple are author intent and are
/// left untouched.
const VECTOR_GRAY_MAX_SPREAD: f32 = 26.0;

/// Map a dark achromatic vector paint to its light counterpart (`#000` →
/// `#fff`, `#333` → `#ccc`) when the SVG paints under a dark used color
/// scheme. Returns `None` when the value must be kept as-is.
///
/// This deliberately only fires for near-gray darks inside SVG paint: raster
/// images, HTML text and saturated brand colors never reach this path, so
/// photos and intentional artwork are preserved while monochrome icons that
/// ship no dark variant stay visible.
fn lighten_dark_vector_gray(value: &str) -> Option<String> {
    let (r, g, b, alpha) = parse_srgb_color(value)?;
    let spread = r.max(g).max(b) - r.min(g).min(b);
    if spread > VECTOR_GRAY_MAX_SPREAD {
        return None;
    }
    if relative_luminance(r, g, b) >= DARK_VECTOR_GRAY_LUMINANCE {
        return None;
    }
    let map = |channel: f32| (255.0 - channel).round().clamp(0.0, 255.0) as u8;
    Some(match alpha {
        Some(alpha) => format!("rgba({}, {}, {}, {})", map(r), map(g), map(b), alpha),
        None => format!("rgb({}, {}, {})", map(r), map(g), map(b)),
    })
}

/// Parse `rgb()`/`rgba()`/`#rgb`/`#rrggbb` into 0-255 channels plus optional
/// alpha. Anything else (keywords beyond what computed style serializes,
/// `none`, `url()`, system colors, wide-gamut `color()`) yields `None` and
/// is preserved untouched.
fn parse_srgb_color(value: &str) -> Option<(f32, f32, f32, Option<f32>)> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        let channels: Vec<u8> = match hex.len() {
            3 => hex
                .chars()
                .map(|digit| u8::from_str_radix(&digit.to_string().repeat(2), 16).ok())
                .collect::<Option<_>>()?,
            6 => (0..3)
                .map(|index| u8::from_str_radix(&hex[2 * index..2 * index + 2], 16).ok())
                .collect::<Option<_>>()?,
            _ => return None,
        };
        return Some((channels[0] as f32, channels[1] as f32, channels[2] as f32, None));
    }
    let (function, arguments) = value.strip_suffix(')')?.split_once('(')?;
    if function != "rgb" && function != "rgba" {
        return None;
    }
    // Modern space-separated syntax is normalized back to commas; `none`
    // components cannot describe a dark gray, so reject them.
    let normalized = arguments.replace('/', ",").replace(' ', ",");
    let mut components = normalized
        .split(',')
        .map(str::trim)
        .filter(|component| !component.is_empty());
    let channel = |component: Option<&str>| {
        let component = component?;
        if component == "none" {
            return None;
        }
        if let Some(percent) = component.strip_suffix('%') {
            Some(percent.trim().parse::<f32>().ok()? * 255.0 / 100.0)
        } else {
            component.parse::<f32>().ok()
        }
    };
    let r = channel(components.next())?;
    let g = channel(components.next())?;
    let b = channel(components.next())?;
    if !(0.0..=255.0).contains(&r) || !(0.0..=255.0).contains(&g) || !(0.0..=255.0).contains(&b) {
        return None;
    }
    let alpha: Option<f32> = match components.next() {
        None => None,
        Some(text) => Some(text.parse::<f32>().ok()?),
    };
    if components.next().is_some() {
        return None;
    }
    Some((r, g, b, alpha))
}

/// sRGB relative luminance per WCAG (channels in 0-255).
fn relative_luminance(r: f32, g: f32, b: f32) -> f32 {
    let linear = |channel: f32| {
        let channel = channel / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

impl SVGSVGElement {
    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGSVGElement {
        SVGSVGElement {
            svggraphicselement: SVGGraphicsElement::new_inherited(local_name, prefix, document),
            uuid: Uuid::new_v4(),
            cached_serialized_data_url: Default::default(),
        }
    }

    #[cfg_attr(crown, allow(crown::unrooted_must_root))]
    pub(crate) fn new(
        cx: &mut js::context::JSContext,
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
        proto: Option<HandleObject>,
    ) -> DomRoot<SVGSVGElement> {
        Node::reflect_node_with_proto(
            cx,
            Box::new(SVGSVGElement::new_inherited(local_name, prefix, document)),
            document,
            proto,
        )
    }

    pub(crate) fn serialize_and_cache_subtree(&self, cx: &mut js::context::JSContext) {
        let document_fragment = self.owner_document().CreateDocumentFragment(cx);
        let cloned_node = Node::clone(
            cx,
            self.upcast(),
            None,
            CloneChildrenFlag::CloneChildren,
            None,
        );
        if document_fragment
            .upcast::<Node>()
            .AppendChild(cx, &cloned_node)
            .is_err()
        {
            error!("Unable to clone SVG tree");
            *self.cached_serialized_data_url.borrow_mut() = Some(Err(()));
            return;
        }

        self.process_use_elements(cx, &cloned_node);
        self.inline_resolved_paint_styles(cx, &cloned_node);

        let Ok(xml_source) = cloned_node.xml_serialize(TraversalScope::IncludeNode) else {
            *self.cached_serialized_data_url.borrow_mut() = Some(Err(()));
            return;
        };

        let xml_source: String = xml_source.into();
        let base64_encoded_source = base64::engine::general_purpose::STANDARD.encode(xml_source);
        let data_url = format!("data:image/svg+xml;base64,{base64_encoded_source}");
        match ServoUrl::parse(&data_url) {
            Ok(url) => *self.cached_serialized_data_url.borrow_mut() = Some(Ok(url)),
            Err(error) => error!("Unable to parse serialized SVG data url: {error}"),
        };
    }

    fn process_use_elements(&self, cx: &mut JSContext, root_node: &Node) {
        for node in root_node.traverse_preorder(ShadowIncluding::No) {
            if let Some(element) = node.downcast::<Element>() &&
                element.local_name() == &local_name!("use")
            {
                self.process_single_use_element(cx, element, root_node)
            }
        }
    }

    /// Materialize the live document's resolved paint values as inline style
    /// on the detached clone, so the serialized standalone copy paints what
    /// the cascade specifies.
    ///
    /// The clone carries no document stylesheets: only presentation attributes
    /// and inline `style` survive serialization. Without this, every
    /// class-driven paint — e.g. Tailwind `dark:fill-white` logos, or
    /// `fill: currentColor` icon systems — rasterizes with the initial values
    /// (typically black), no matter what the page's stylesheets resolve. The
    /// values are read from the live elements via `getComputedStyle`, hence
    /// already reflect inheritance, CSS variables, `currentColor`,
    /// `light-dark()` and media queries. Only paint-affecting longhands are
    /// copied; layout and all other properties are untouched.
    fn inline_resolved_paint_styles(&self, cx: &mut JSContext, cloned_root: &Node) {
        let window = self.owner_window();
        let darken_icons = self.subtree_uses_dark_scheme(cx);
        // Root both walks so neither tree can be collected mid-pass.
        let live_nodes: Vec<DomRoot<Node>> = self
            .upcast::<Node>()
            .traverse_preorder(ShadowIncluding::No)
            .collect();
        let cloned_nodes: Vec<DomRoot<Node>> = cloned_root
            .traverse_preorder(ShadowIncluding::No)
            .collect();
        for (live_node, cloned_node) in live_nodes.iter().zip(cloned_nodes.iter()) {
            let (Some(live_element), Some(cloned_element)) = (
                live_node.downcast::<Element>(),
                cloned_node.downcast::<Element>(),
            ) else {
                continue;
            };
            let computed = window.GetComputedStyle(cx, live_element, None);
            let mut inline = String::new();
            for property in SERIALIZED_SVG_PAINT_PROPERTIES {
                let raw = String::from(computed.GetPropertyValue(DOMString::from(*property)));
                let value = raw.trim();
                if value.is_empty() {
                    continue;
                }
                let value = if darken_icons && is_remappable_paint_property(property) {
                    lighten_dark_vector_gray(value).unwrap_or_else(|| value.to_owned())
                } else {
                    value.to_owned()
                };
                inline.push_str(property);
                inline.push(':');
                inline.push_str(&value);
                inline.push(';');
            }
            if inline.is_empty() {
                continue;
            }
            cloned_element.set_attribute(
                cx,
                &local_name!("style"),
                AttrValue::String(inline),
            );
        }
    }

    /// Whether this SVG paints under a dark used color scheme, per CSS Color
    /// Adjustment: the used scheme is the preferred (document, else embedder)
    /// theme when the declared `color-scheme` list contains it, otherwise the
    /// first declared scheme (`normal`/unset means light).
    fn subtree_uses_dark_scheme(&self, cx: &mut JSContext) -> bool {
        let document = self.owner_document();
        let window = self.owner_window();
        let preferred_dark = matches!(
            document.theme().unwrap_or(window.embedder_theme()),
            Theme::Dark
        );
        let declared = document
            .GetDocumentElement()
            .map(|root| {
                String::from(
                    window
                        .GetComputedStyle(cx, &root, None)
                        .GetPropertyValue(DOMString::from("color-scheme")),
                )
            })
            .unwrap_or_default();
        let schemes: Vec<&str> = declared
            .split_whitespace()
            .filter(|scheme| !scheme.eq_ignore_ascii_case("only"))
            .collect();
        if preferred_dark {
            schemes
                .iter()
                .any(|scheme| scheme.eq_ignore_ascii_case("dark"))
        } else {
            schemes
                .first()
                .is_some_and(|first| first.eq_ignore_ascii_case("dark"))
        }
    }

    fn process_single_use_element(
        &self,
        cx: &mut JSContext,
        use_element: &Element,
        root_node: &Node,
    ) {
        let href = use_element.get_string_attribute(&local_name!("href"));
        let Some(id_string) = href.str().strip_prefix("#").map(DOMString::from) else {
            return;
        };

        let document = self.upcast::<Node>().owner_doc();
        let Some(referenced_element) = document.GetElementById(cx, id_string) else {
            return;
        };
        let referenced_node = referenced_element.upcast::<Node>();

        // Don't use this node if it doesn't have an `<svg>` ancestor.
        if !referenced_node
            .inclusive_ancestors_unrooted(cx.no_gc(), ShadowIncluding::No)
            .any(|ancestor| ancestor.is::<SVGSVGElement>())
        {
            return;
        };

        // Don't use this node if it already exists within the same `<svg>` element.
        if referenced_node
            .inclusive_ancestors_unrooted(cx.no_gc(), ShadowIncluding::No)
            .any(|ancestor| *ancestor == self.upcast())
        {
            return;
        };

        let cloned_node = Node::clone(
            cx,
            referenced_node,
            None,
            CloneChildrenFlag::CloneChildren,
            None,
        );
        let _ = root_node.AppendChild(cx, &cloned_node);
    }

    fn invalidate_cached_serialized_subtree_and_rasterization_result(&self, no_gc: &NoGC) {
        let owner_window = self.owner_window();
        owner_window
            .image_cache()
            .evict_rasterized_image(&self.uuid);
        if let Some(Ok(url)) = &*self.cached_serialized_data_url.borrow() {
            owner_window.layout_mut().remove_cached_image(url);
            owner_window.image_cache().evict_completed_image(
                url,
                owner_window.origin().immutable(),
                &None,
            );
        }

        *self.cached_serialized_data_url.borrow_mut() = None;
        self.upcast::<Node>().dirty(no_gc, NodeDamage::Other);
    }
}

impl<'dom> LayoutDom<'dom, SVGSVGElement> {
    pub(crate) fn data(self) -> SVGElementData<'dom> {
        let svg_id = self.unsafe_get().uuid;
        let element = self.upcast::<Element>();
        let width = element.get_attr_for_layout(&ns!(), &local_name!("width"));
        let height = element.get_attr_for_layout(&ns!(), &local_name!("height"));
        let view_box = element.get_attr_for_layout(&ns!(), &local_name!("viewBox"));
        SVGElementData {
            source: self
                .unsafe_get()
                .cached_serialized_data_url
                .borrow()
                .clone(),
            width,
            height,
            view_box,
            svg_id,
        }
    }
}

impl VirtualMethods for SVGSVGElement {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<SVGGraphicsElement>() as &dyn VirtualMethods)
    }

    fn attribute_mutated(
        &self,
        cx: &mut js::context::JSContext,
        attr: AttrRef<'_>,
        mutation: AttributeMutation,
    ) {
        self.super_type()
            .unwrap()
            .attribute_mutated(cx, attr, mutation);

        self.invalidate_cached_serialized_subtree_and_rasterization_result(cx.no_gc());
    }

    fn attribute_affects_presentational_hints(&self, attr: AttrRef<'_>) -> bool {
        match attr.local_name() {
            &local_name!("width") | &local_name!("height") => true,
            _ => self
                .super_type()
                .unwrap()
                .attribute_affects_presentational_hints(attr),
        }
    }

    fn parse_plain_attribute(&self, name: &LocalName, value: DOMString) -> AttrValue {
        match *name {
            local_name!("width") | local_name!("height") => {
                let value = &value.str();
                let parser = &mut Parser::new(value);
                let doc = self.owner_document();
                let url = doc.url().into_url().into();
                let context = ParserContext::new(
                    Origin::Author,
                    &url,
                    None,
                    ParsingMode::ALLOW_UNITLESS_LENGTH,
                    doc.quirks_mode(),
                    /* namespaces = */ Default::default(),
                    None,
                    None,
                    /* attr_taint = */ Default::default(),
                );
                let val = LengthPercentage::parse_quirky(
                    &context,
                    parser,
                    style::values::specified::AllowQuirks::Always,
                );
                AttrValue::LengthPercentage(value.to_string(), val.ok())
            },
            _ => self
                .super_type()
                .unwrap()
                .parse_plain_attribute(name, value),
        }
    }

    fn children_changed(&self, cx: &mut JSContext, mutation: &ChildrenMutation) {
        if let Some(super_type) = self.super_type() {
            super_type.children_changed(cx, mutation);
        }

        self.invalidate_cached_serialized_subtree_and_rasterization_result(cx.no_gc());
    }

    fn unbind_from_tree(&self, cx: &mut js::context::JSContext, context: &UnbindContext<'_>) {
        if let Some(s) = self.super_type() {
            s.unbind_from_tree(cx, context);
        }

        self.invalidate_cached_serialized_subtree_and_rasterization_result(cx.no_gc());
    }
}
