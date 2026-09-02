/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use app_units::Au;
use euclid::Size2D;
use style::Zero;
use style::color::mix::{ColorInterpolationMethod, ColorMixItem, HueInterpolationMethod, mix_many};
use style::color::{AbsoluteColor, ColorSpace};
use style::properties::ComputedValues;
use style::values::computed::image::{EndingShape, Gradient, LineDirection};
use style::values::computed::{Angle, AngleOrPercentage, Color, LengthPercentage, Position};
use style::values::generics::color::ColorMixFlags;
use style::values::generics::image::{
    Circle, ColorStop, Ellipse, GradientFlags, GradientItem, ShapeExtent,
};
use webrender_api::units::LayoutPixel;
use webrender_api::{
    self as wr, ConicGradient as WebRenderConicGradient, ExtendMode,
    Gradient as WebRenderLinearGradient, RadialGradient as WebRenderRadialGradient, units,
};

pub(super) enum WebRenderGradient {
    Linear(WebRenderLinearGradient),
    Radial(WebRenderRadialGradient),
    Conic(WebRenderConicGradient),
}

fn linear_gradient_line(
    line_direction: &LineDirection,
    gradient_box: Size2D<f32, LayoutPixel>,
) -> (units::LayoutPoint, units::LayoutPoint, f32) {
    use style::values::specified::position::HorizontalPositionKeyword::*;
    use style::values::specified::position::VerticalPositionKeyword::*;
    use units::LayoutVector2D as Vec2;

    let direction = match line_direction {
        LineDirection::Horizontal(Right) => Vec2::new(1., 0.),
        LineDirection::Vertical(Top) => Vec2::new(0., -1.),
        LineDirection::Horizontal(Left) => Vec2::new(-1., 0.),
        LineDirection::Vertical(Bottom) => Vec2::new(0., 1.),
        LineDirection::Angle(angle) => {
            let radians = angle.radians();
            Vec2::new(radians.sin(), -radians.cos())
        },
        LineDirection::Corner(horizontal, vertical) => {
            let x = match horizontal {
                Right => gradient_box.height,
                Left => -gradient_box.height,
            };
            let y = match vertical {
                Top => -gradient_box.width,
                Bottom => gradient_box.width,
            };
            Vec2::new(x, y).normalize()
        },
    };

    let gradient_line_length =
        (gradient_box.width * direction.x).abs() + (gradient_box.height * direction.y).abs();
    let half_gradient_line = direction * (gradient_line_length / 2.);
    let center = (gradient_box / 2.).to_vector().to_point();
    (
        center - half_gradient_line,
        center + half_gradient_line,
        gradient_line_length,
    )
}

/// Samples a linear CSS gradient at a point in its gradient box.
///
/// WebRender currently accepts a single colour for a glyph run, so this is used
/// to approximate `background-clip: text` at glyph granularity while retaining
/// CSS Images gradient geometry, stop fix-up, repeating behaviour, colour-space
/// conversion, and premultiplied-alpha interpolation.
pub(super) fn sample_linear(
    style: &ComputedValues,
    gradient: &Gradient,
    gradient_box: Size2D<f32, LayoutPixel>,
    point: units::LayoutPoint,
) -> Option<wr::ColorF> {
    let Gradient::Linear {
        items,
        direction,
        color_interpolation_method,
        flags,
        compat_mode: _,
    } = gradient
    else {
        return None;
    };

    let (start_point, end_point, gradient_line_length) =
        linear_gradient_line(direction, gradient_box);
    if gradient_line_length <= f32::EPSILON {
        return None;
    }

    let extend_mode = if flags.contains(GradientFlags::REPEATING) {
        wr::ExtendMode::Repeat
    } else {
        wr::ExtendMode::Clamp
    };
    let mut color_stops =
        gradient_items_to_color_stops(style, items, Au::from_f32_px(gradient_line_length));
    if color_stops.is_empty() {
        return None;
    }
    let stops = create_webrender_stops(&mut color_stops, color_interpolation_method, extend_mode);
    if stops.is_empty() {
        return None;
    }

    let line = end_point - start_point;
    let line_length_squared = line.x * line.x + line.y * line.y;
    if line_length_squared <= f32::EPSILON {
        return stops.first().map(|stop| stop.color);
    }
    let from_start = point - start_point;
    let mut offset = (from_start.x * line.x + from_start.y * line.y) / line_length_squared;
    offset = match extend_mode {
        wr::ExtendMode::Repeat => offset.rem_euclid(1.0),
        wr::ExtendMode::Clamp => offset,
    };

    if offset <= stops[0].offset {
        return Some(stops[0].color);
    }
    for pair in stops.windows(2) {
        let start = &pair[0];
        let end = &pair[1];
        if offset > end.offset {
            continue;
        }
        let span = end.offset - start.offset;
        let amount = if span.abs() <= f32::EPSILON {
            1.0
        } else {
            ((offset - start.offset) / span).clamp(0.0, 1.0)
        };
        let alpha = start.color.a + (end.color.a - start.color.a) * amount;
        let premultiplied = |channel_start: f32, channel_end: f32| {
            let start = channel_start * start.color.a;
            let end = channel_end * end.color.a;
            if alpha <= f32::EPSILON {
                0.0
            } else {
                (start + (end - start) * amount) / alpha
            }
        };
        return Some(wr::ColorF::new(
            premultiplied(start.color.r, end.color.r),
            premultiplied(start.color.g, end.color.g),
            premultiplied(start.color.b, end.color.b),
            alpha,
        ));
    }
    stops.last().map(|stop| stop.color)
}

pub(super) fn build(
    style: &ComputedValues,
    gradient: &Gradient,
    size: Size2D<f32, LayoutPixel>,
    builder: &mut super::DisplayListBuilder,
) -> WebRenderGradient {
    match gradient {
        Gradient::Linear {
            items,
            direction,
            color_interpolation_method,
            flags,
            compat_mode: _,
        } => build_linear(
            style,
            items,
            direction,
            color_interpolation_method,
            *flags,
            size,
            builder,
        ),
        Gradient::Radial {
            shape,
            position,
            color_interpolation_method,
            items,
            flags,
            compat_mode: _,
        } => build_radial(
            style,
            items,
            shape,
            position,
            color_interpolation_method,
            *flags,
            size,
            builder,
        ),
        Gradient::Conic {
            angle,
            position,
            color_interpolation_method,
            items,
            flags,
        } => build_conic(
            style,
            *angle,
            position,
            color_interpolation_method,
            items,
            *flags,
            size,
            builder,
        ),
    }
}

/// <https://drafts.csswg.org/css-images-3/#linear-gradients>
pub(super) fn build_linear(
    style: &ComputedValues,
    items: &[GradientItem<Color, LengthPercentage>],
    line_direction: &LineDirection,
    color_interpolation_method: &ColorInterpolationMethod,
    flags: GradientFlags,
    gradient_box: Size2D<f32, LayoutPixel>,
    builder: &mut super::DisplayListBuilder,
) -> WebRenderGradient {
    let (start_point, end_point, gradient_line_length) =
        linear_gradient_line(line_direction, gradient_box);

    let extend_mode = if flags.contains(GradientFlags::REPEATING) {
        wr::ExtendMode::Repeat
    } else {
        wr::ExtendMode::Clamp
    };

    let mut color_stops =
        gradient_items_to_color_stops(style, items, Au::from_f32_px(gradient_line_length));
    let stops = create_webrender_stops(&mut color_stops, color_interpolation_method, extend_mode);

    WebRenderGradient::Linear(builder.wr().create_gradient(
        start_point,
        end_point,
        stops,
        extend_mode,
    ))
}

/// <https://drafts.csswg.org/css-images-3/#radial-gradients>
#[expect(clippy::too_many_arguments)]
pub(super) fn build_radial(
    style: &ComputedValues,
    items: &[GradientItem<Color, LengthPercentage>],
    shape: &EndingShape,
    center: &Position,
    color_interpolation_method: &ColorInterpolationMethod,
    flags: GradientFlags,
    gradient_box: Size2D<f32, LayoutPixel>,
    builder: &mut super::DisplayListBuilder,
) -> WebRenderGradient {
    let center = units::LayoutPoint::new(
        center
            .horizontal
            .to_used_value(Au::from_f32_px(gradient_box.width))
            .to_f32_px(),
        center
            .vertical
            .to_used_value(Au::from_f32_px(gradient_box.height))
            .to_f32_px(),
    );
    let radii = match shape {
        EndingShape::Circle(circle) => {
            let radius = match circle {
                Circle::Radius(r) => r.0.px(),
                Circle::Extent(extent) => match extent {
                    ShapeExtent::ClosestSide | ShapeExtent::Contain => {
                        let vec = abs_vector_to_corner(gradient_box, center, f32::min);
                        vec.x.min(vec.y)
                    },
                    ShapeExtent::FarthestSide => {
                        let vec = abs_vector_to_corner(gradient_box, center, f32::max);
                        vec.x.max(vec.y)
                    },
                    ShapeExtent::ClosestCorner => {
                        abs_vector_to_corner(gradient_box, center, f32::min).length()
                    },
                    ShapeExtent::FarthestCorner | ShapeExtent::Cover => {
                        abs_vector_to_corner(gradient_box, center, f32::max).length()
                    },
                },
            };
            units::LayoutSize::new(radius, radius)
        },
        EndingShape::Ellipse(Ellipse::Radii(rx, ry)) => units::LayoutSize::new(
            rx.0.to_used_value(Au::from_f32_px(gradient_box.width))
                .to_f32_px(),
            ry.0.to_used_value(Au::from_f32_px(gradient_box.height))
                .to_f32_px(),
        ),
        EndingShape::Ellipse(Ellipse::Extent(extent)) => match extent {
            ShapeExtent::ClosestSide | ShapeExtent::Contain => {
                abs_vector_to_corner(gradient_box, center, f32::min).to_size()
            },
            ShapeExtent::FarthestSide => {
                abs_vector_to_corner(gradient_box, center, f32::max).to_size()
            },
            ShapeExtent::ClosestCorner => {
                abs_vector_to_corner(gradient_box, center, f32::min).to_size() *
                    (std::f32::consts::FRAC_1_SQRT_2 * 2.0)
            },
            ShapeExtent::FarthestCorner | ShapeExtent::Cover => {
                abs_vector_to_corner(gradient_box, center, f32::max).to_size() *
                    (std::f32::consts::FRAC_1_SQRT_2 * 2.0)
            },
        },
    };

    /// Returns the distance to the nearest or farthest sides in the respective dimension,
    /// depending on `select`.
    fn abs_vector_to_corner(
        gradient_box: units::LayoutSize,
        center: units::LayoutPoint,
        select: impl Fn(f32, f32) -> f32,
    ) -> units::LayoutVector2D {
        let left = center.x.abs();
        let top = center.y.abs();
        let right = (gradient_box.width - center.x).abs();
        let bottom = (gradient_box.height - center.y).abs();
        units::LayoutVector2D::new(select(left, right), select(top, bottom))
    }

    // “The gradient line’s starting point is at the center of the gradient,
    //  and it extends toward the right, with the ending point on the point
    //  where the gradient line intersects the ending shape.”
    let gradient_line_length = radii.width;

    let extend_mode = if flags.contains(GradientFlags::REPEATING) {
        wr::ExtendMode::Repeat
    } else {
        wr::ExtendMode::Clamp
    };

    let mut color_stops =
        gradient_items_to_color_stops(style, items, Au::from_f32_px(gradient_line_length));
    let stops = create_webrender_stops(&mut color_stops, color_interpolation_method, extend_mode);

    WebRenderGradient::Radial(builder.wr().create_radial_gradient(
        center,
        radii,
        stops,
        extend_mode,
    ))
}

/// <https://drafts.csswg.org/css-images-4/#conic-gradients>
#[expect(clippy::too_many_arguments)]
fn build_conic(
    style: &ComputedValues,
    angle: Angle,
    center: &Position,
    color_interpolation_method: &ColorInterpolationMethod,
    items: &[GradientItem<Color, AngleOrPercentage>],
    flags: GradientFlags,
    gradient_box: Size2D<f32, LayoutPixel>,
    builder: &mut super::DisplayListBuilder<'_>,
) -> WebRenderGradient {
    let center = units::LayoutPoint::new(
        center
            .horizontal
            .to_used_value(Au::from_f32_px(gradient_box.width))
            .to_f32_px(),
        center
            .vertical
            .to_used_value(Au::from_f32_px(gradient_box.height))
            .to_f32_px(),
    );

    let extend_mode = if flags.contains(GradientFlags::REPEATING) {
        wr::ExtendMode::Repeat
    } else {
        wr::ExtendMode::Clamp
    };

    let mut color_stops = conic_gradient_items_to_color_stops(style, items);
    let stops = create_webrender_stops(&mut color_stops, color_interpolation_method, extend_mode);

    WebRenderGradient::Conic(builder.wr().create_conic_gradient(
        center,
        angle.radians(),
        stops,
        extend_mode,
    ))
}

fn conic_gradient_items_to_color_stops(
    style: &ComputedValues,
    items: &[GradientItem<Color, AngleOrPercentage>],
) -> Vec<ColorStop<AbsoluteColor, f32>> {
    // Remove color transititon hints, which are not supported yet.
    // https://drafts.csswg.org/css-images-4/#color-transition-hint
    //
    // This gives an approximation of the gradient that might be visibly wrong,
    // but maybe better than not parsing that value at all?
    // It’s debatble whether that’s better or worse
    // than not parsing and allowing authors to set a fallback.
    // Either way, the best outcome is to add support.
    // Gecko does so by approximating the non-linear interpolation
    // by up to 10 piece-wise linear segments (9 intermediate color stops)
    items
        .iter()
        .filter_map(|item| {
            match item {
                GradientItem::SimpleColorStop(color) => Some(ColorStop {
                    color: style.resolve_color(color),
                    position: None,
                }),
                GradientItem::ComplexColorStop { color, position } => Some(ColorStop {
                    color: style.resolve_color(color),
                    position: match position {
                        AngleOrPercentage::Percentage(percentage) => Some(percentage.0),
                        AngleOrPercentage::Angle(angle) => Some(angle.degrees() / 360.),
                    },
                }),
                // FIXME: approximate like in:
                // https://searchfox.org/mozilla-central/rev/f98dad153b59a985efd4505912588d4651033395/layout/painting/nsCSSRenderingGradients.cpp#315-391
                GradientItem::InterpolationHint(_) => None,
            }
        })
        .collect()
}

fn gradient_items_to_color_stops(
    style: &ComputedValues,
    items: &[GradientItem<Color, LengthPercentage>],
    gradient_line_length: Au,
) -> Vec<ColorStop<AbsoluteColor, f32>> {
    // Remove color transititon hints, which are not supported yet.
    // https://drafts.csswg.org/css-images-4/#color-transition-hint
    //
    // This gives an approximation of the gradient that might be visibly wrong,
    // but maybe better than not parsing that value at all?
    // It’s debatble whether that’s better or worse
    // than not parsing and allowing authors to set a fallback.
    // Either way, the best outcome is to add support.
    // Gecko does so by approximating the non-linear interpolation
    // by up to 10 piece-wise linear segments (9 intermediate color stops)
    items
        .iter()
        .filter_map(|item| {
            match item {
                GradientItem::SimpleColorStop(color) => Some(ColorStop {
                    color: style.resolve_color(color),
                    position: None,
                }),
                GradientItem::ComplexColorStop { color, position } => Some(ColorStop {
                    color: style.resolve_color(color),
                    position: Some(if gradient_line_length.is_zero() {
                        0.
                    } else {
                        position
                            .to_used_value(gradient_line_length)
                            .scale_by(1. / gradient_line_length.to_f32_px())
                            .to_f32_px()
                    }),
                }),
                // FIXME: approximate like in:
                // https://searchfox.org/mozilla-central/rev/f98dad153b59a985efd4505912588d4651033395/layout/painting/nsCSSRenderingGradients.cpp#315-391
                GradientItem::InterpolationHint(_) => None,
            }
        })
        .collect()
}

fn create_webrender_stops(
    stops: &mut [ColorStop<AbsoluteColor, f32>],
    interpolation_method: &ColorInterpolationMethod,
    extend_mode: ExtendMode,
) -> Vec<wr::GradientStop> {
    let stops = fixup_stops(stops);
    if interpolation_method.space != ColorSpace::Srgb {
        return interpolate_gradient_stop_colors(&stops, interpolation_method, extend_mode);
    }

    stops
        .iter()
        .map(|stop| wr::GradientStop {
            color: super::rgba(stop.color),
            offset: stop.position,
        })
        .collect()
}

#[derive(Clone, Copy)]
struct UsedColorStop {
    color: AbsoluteColor,
    position: f32,
}

/// <https://drafts.csswg.org/css-images-4/#color-stop-fixup>
fn fixup_stops(stops: &mut [ColorStop<AbsoluteColor, f32>]) -> Vec<UsedColorStop> {
    assert!(!stops.is_empty());

    // https://drafts.csswg.org/css-images-4/#color-stop-fixup
    if let first_position @ None = &mut stops.first_mut().unwrap().position {
        *first_position = Some(0.);
    }
    if let last_position @ None = &mut stops.last_mut().unwrap().position {
        *last_position = Some(1.);
    }

    let mut iter = stops.iter_mut();
    let mut max_so_far = iter.next().unwrap().position.unwrap();
    for stop in iter {
        if let Some(position) = &mut stop.position {
            if *position < max_so_far {
                *position = max_so_far
            } else {
                max_so_far = *position
            }
        }
    }

    let mut used_color_stops = Vec::with_capacity(stops.len());
    let mut iter = stops.iter().enumerate();
    let (_, first) = iter.next().unwrap();
    let first_stop_position = first.position.unwrap();
    used_color_stops.push(UsedColorStop {
        position: first_stop_position,
        color: first.color,
    });
    if stops.len() == 1 {
        used_color_stops.push(used_color_stops[0]);
    }

    let mut last_positioned_stop_index = 0;
    let mut last_positioned_stop_position = first_stop_position;
    for (i, stop) in iter {
        if let Some(position) = stop.position {
            let step_count = i - last_positioned_stop_index;
            if step_count > 1 {
                let step = (position - last_positioned_stop_position) / step_count as f32;
                for j in 1..step_count {
                    let color = stops[last_positioned_stop_index + j].color;
                    let position = last_positioned_stop_position + j as f32 * step;
                    used_color_stops.push(UsedColorStop { position, color })
                }
            }
            last_positioned_stop_index = i;
            last_positioned_stop_position = position;
            used_color_stops.push(UsedColorStop {
                position,
                color: stop.color,
            })
        }
    }

    used_color_stops
}

/// This is a port of Gecko's WrColorStopInterpolator:
///
/// See
/// <https://searchfox.org/firefox-main/rev/4b851f6b592ecf1112ee47dd25e8de28c892ad67/layout/painting/nsCSSRenderingGradients.cpp#1200>
fn interpolate_gradient_stop_colors(
    stops: &[UsedColorStop],
    interpolation_method: &ColorInterpolationMethod,
    extend_mode: wr::ExtendMode,
) -> Vec<wr::GradientStop> {
    // This could be made tunable, but at 1.0/128 the error is largely
    // irrelevant, as WebRender re-encodes it to 128 pairs of stops.
    //
    // Note that we don't attempt to place the positions of these stops
    // precisely at intervals, we just add this many extra stops across the
    // range where it is convenient.
    const FULL_RANGE_EXTRA_STOPS: usize = 128;

    // This indicates that we want to extend the end position on the last stop,
    // which only matters if this is a CSS non-repeating gradient with
    // StyleHueInterpolationMethod::Longer (only valid for hsl/hwb/lch/oklch).
    //
    // For the specific case of longer hue interpolation on a CSS non-repeating
    // gradient, we have to pretend there is another stop at position=1.0 that
    // duplicates the last stop, this is probably only used for things like a
    // color wheel.  No such problem for SVG as it doesn't have that complexity.
    let extend = extend_mode == wr::ExtendMode::Clamp &&
        interpolation_method.hue == HueInterpolationMethod::Longer;

    // We always emit at least two stops (start and end) for each input stop,
    // which avoids ambiguity with incomplete oklch/lch/hsv/hsb color stops for
    // the last stop pair, where the last color stop can't be interpreted on its
    // own because it actually depends on the previous stop.
    let mut output = Vec::with_capacity(stops.len() * 2 + FULL_RANGE_EXTRA_STOPS);

    // This loop intentionally iterates extra stops at the beginning and end
    // if extending was requested, or in the degenerate case where only one
    // color stop was specified.
    let extend = extend || stops.len() == 1;
    let mut iter_stops = stops.len() - 1;
    if extend {
        iter_stops += 2;
    }

    for index in 0..iter_stops {
        let this_index = if extend {
            index.saturating_sub(1)
        } else {
            index
        };

        let next_index = if extend && (index == iter_stops - 1 || index == 0) {
            this_index
        } else {
            this_index + 1
        };

        let start = &stops[this_index];
        let end = &stops[next_index];
        let mut start_position = start.position;
        let mut end_position = end.position;

        // For CSS non-repeating gradients with longer hue specified, we have to
        // pretend there is a stop beyond the last stop, and one before the first.
        // This is never the case on SVG gradients as they only use shorter hue.
        //
        // See https://bugzilla.mozilla.org/show_bug.cgi?id=1885716 for more info.
        let mut extra_stops = 0;
        if extend {
            // If we're extending, we just need a single new stop, which will
            // duplicate the end being extended; do not create interpolated stops
            // within the extension area!
            if index == 0 {
                start_position = start_position.min(0.0);
                extra_stops = 1;
            }
            if index == iter_stops - 1 {
                end_position = end_position.max(1.0);
                extra_stops = 1;
            }
        }

        if extra_stops == 0 {
            // Within the actual gradient range, figure out how many extra stops
            // to use for this section of the gradient.
            extra_stops = (end_position * FULL_RANGE_EXTRA_STOPS as f32).floor() as u32;
            extra_stops = extra_stops.clamp(1, FULL_RANGE_EXTRA_STOPS as u32);
        }

        let step = 1.0 / (extra_stops as f32);
        for extra_stop in 0..=extra_stops {
            let progress = (extra_stop as f32) * step;
            let position = start_position + progress * (end_position - start_position);

            let start_color = start.color;
            let end_color = end.color;
            let color = mix_many(
                *interpolation_method,
                [
                    ColorMixItem::new(start_color, 1.0 - progress),
                    ColorMixItem::new(end_color, progress),
                ],
                ColorMixFlags::empty(),
            );

            output.push(wr::GradientStop {
                color: super::rgba(color),
                offset: position,
            });
        }
    }

    output
}
