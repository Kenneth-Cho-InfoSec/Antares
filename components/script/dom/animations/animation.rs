/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use js::context::JSContext;
use js::rust::HandleObject;
use script_bindings::codegen::GenericBindings::AnimationBinding::AnimationPlayState;
use script_bindings::codegen::GenericBindings::DocumentBinding::DocumentMethods;
use script_bindings::codegen::GenericBindings::WindowBinding::WindowMethods;
use script_bindings::inheritance::Castable;
use script_bindings::reflector::reflect_dom_object_with_proto;
use script_bindings::root::DomRoot;
use std::rc::Rc;

use crate::dom::animationeffect::AnimationEffect;
use crate::dom::bindings::codegen::Bindings::AnimationBinding::AnimationMethods;
use script_bindings::cell::DomRefCell;

use crate::dom::bindings::root::MutNullableDom;
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::promise::{Promise, RootedPromise, TracedPromise};
use crate::dom::types::AnimationTimeline;
use crate::dom::window::Window;

/// <https://drafts.csswg.org/web-animations-1/#animation>
#[dom_struct]
pub(crate) struct Animation {
    event_target: EventTarget,

    /// <https://drafts.csswg.org/web-animations-1/#timeline>
    timeline: MutNullableDom<AnimationTimeline>,

    /// <https://drafts.csswg.org/web-animations-1/#animation-associated-effect>
    associated_effect: MutNullableDom<AnimationEffect>,

    /// Pre-resolved `ready` promise. Stub animations are immediately ready.
    ready_promise: DomRefCell<Option<TracedPromise>>,
    /// Pre-resolved `finished` promise. Stub animations are immediately finished.
    finished_promise: DomRefCell<Option<TracedPromise>>,
}

impl Animation {
    pub(crate) fn new_inherited() -> Self {
        Self {
            event_target: EventTarget::new_inherited(),
            timeline: Default::default(),
            associated_effect: Default::default(),
            ready_promise: Default::default(),
            finished_promise: Default::default(),
        }
    }

    fn new_with_proto(
        cx: &mut JSContext,
        global: &GlobalScope,
        proto: Option<HandleObject>,
    ) -> DomRoot<Self> {
        reflect_dom_object_with_proto(cx, Box::new(Self::new_inherited()), global, proto)
    }

    pub(crate) fn new(cx: &mut JSContext, global: &GlobalScope) -> DomRoot<Self> {
        let animation = Self::new_with_proto(cx, global, None);
        // Stub animations never tick, so resolve both promises immediately.
        *animation.ready_promise.borrow_mut() = Some(
            Promise::new_resolved_rooted(cx, global, animation.clone()).to_traced(),
        );
        *animation.finished_promise.borrow_mut() = Some(
            Promise::new_resolved_rooted(cx, global, animation.clone()).to_traced(),
        );
        animation
    }

    /// <https://drafts.csswg.org/web-animations-1/#animation-set-the-timeline-of-an-animation>
    fn set_the_timeline(&self, timeline: &AnimationTimeline) {
        // FIXME: Implement this fully
        self.timeline.set(Some(timeline));
    }

    /// <https://drafts.csswg.org/web-animations-1/#animation-set-the-associated-effect-of-an-animation>
    fn set_the_associated_effect(&self, effect: Option<&AnimationEffect>) {
        // FIXME: Implement this fully
        self.associated_effect.set(effect);
    }
}

impl AnimationMethods<crate::DomTypeHolder> for Animation {
    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-animation>
    fn Constructor(
        cx: &mut JSContext,
        window: &Window,
        _object: Option<HandleObject>,
        effect: Option<&AnimationEffect>,
    ) -> DomRoot<Self> {
        let document = window.Document();

        // Step 1. Let animation be a new Animation object.
        let animation = Animation::new(cx, window.upcast());

        // Step 2. Run the procedure to set the timeline of an animation on animation passing timeline
        // as the new timeline; or, if the timeline argument is missing, passing the default document
        // timeline of the Document associated with the Window that is the current global object.
        // TODO: We don't suppor the timeline argument yet.
        animation.set_the_timeline(document.Timeline().upcast());

        // Step 3. Run the procedure to set the associated effect of an animation on animation passing
        // source as the new effect.
        animation.set_the_associated_effect(effect);

        animation
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-effect>
    fn GetEffect(&self) -> Option<DomRoot<AnimationEffect>> {
        self.associated_effect.get()
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-effect>
    fn SetEffect(&self, effect: Option<&AnimationEffect>) {
        // > Setting this attribute updates the object’s associated effect using
        //> the procedure to set the associated effect of an animation.
        self.set_the_associated_effect(effect);
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-playstate>
    ///
    /// Compat stub: keyframe effects are constructed but never ticked, so every
    /// animation is immediately finished. This lets sites calling
    /// `element.animate()` proceed instead of throwing.
    fn PlayState(&self) -> AnimationPlayState {
        AnimationPlayState::Finished
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-ready>
    fn Ready(&self) -> RootedPromise {
        self.ready_promise
            .borrow()
            .as_ref()
            .expect("ready promise initialized at construction")
            .root()
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-finished>
    fn Finished(&self) -> RootedPromise {
        self.finished_promise
            .borrow()
            .as_ref()
            .expect("finished promise initialized at construction")
            .root()
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-cancel>
    fn Cancel(&self) {
        // No-op: effects are never ticked.
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-finish>
    fn Finish(&self) {
        // No-op: already finished.
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-play>
    fn Play(&self) {
        // No-op: effects are never ticked.
    }

    /// <https://drafts.csswg.org/web-animations-1/#dom-animation-pause>
    fn Pause(&self) {
        // No-op: effects are never ticked.
    }
}
