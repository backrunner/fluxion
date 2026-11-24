//! Tooltips follow the current hitbox rather than the bounds captured when a
//! hover timer started. This matters for animated panes and virtual list rows.
use gpui::{prelude::*, *};
use gpui_component::{Disableable, button::Button, tooltip::Tooltip};
use std::{cell::RefCell, rc::Rc, time::Duration};

pub trait HoverTooltipExt: IntoElement + Sized {
    fn hover_tooltip(
        self,
        id: impl Into<ElementId>,
        text: impl Into<SharedString>,
    ) -> HoverTooltip<Self> {
        HoverTooltip {
            inner: self,
            id: id.into(),
            text: text.into(),
        }
    }
}
impl<E: IntoElement> HoverTooltipExt for E {}

pub struct HoverTooltip<E> {
    inner: E,
    id: ElementId,
    text: SharedString,
}
impl<E: Styled> Styled for HoverTooltip<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}
impl HoverTooltip<Button> {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.inner = self.inner.on_click(handler);
        self
    }
}
impl<E: IntoElement> IntoElement for HoverTooltip<E> {
    type Element = TooltipElement;
    fn into_element(self) -> Self::Element {
        TooltipElement {
            inner: self.inner.into_any_element(),
            id: self.id,
            text: self.text,
        }
    }
}

#[derive(Default)]
pub struct HoverState {
    bounds: Bounds<Pixels>,
    text: SharedString,
    pending: Option<Task<()>>,
    visible: Option<AnyView>,
    anchor: Point<Pixels>,
    suppressed: bool,
    hitbox: Option<Hitbox>,
    keystrokes: Option<Subscription>,
}
impl HoverState {
    fn clear(&mut self) {
        self.pending = None;
        self.visible = None;
    }
}

pub struct TooltipElement {
    inner: AnyElement,
    id: ElementId,
    text: SharedString,
}
impl IntoElement for TooltipElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TooltipElement {
    type RequestLayoutState = ();
    type PrepaintState = (Rc<RefCell<HoverState>>, Hitbox);
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.inner.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner.prepaint(window, cx);
        let bounds = bounds.intersect(&window.content_mask().bounds);
        let state =
            window.with_element_state(id.unwrap(), |state: Option<Rc<RefCell<HoverState>>>, _| {
                let state = state.unwrap_or_default();
                (state.clone(), state)
            });
        let mut hover = state.borrow_mut();
        if hover.keystrokes.is_none() {
            let weak = Rc::downgrade(&state);
            let owner = window.window_handle();
            hover.keystrokes = Some(cx.observe_keystrokes(move |_, window, _| {
                if window.window_handle() == owner
                    && let Some(state) = weak.upgrade()
                {
                    dismiss(&state, window);
                }
            }));
        }
        // Cancel both pending and visible hints when layout or content changes.
        // A new pointer movement is required before a moved control can show one.
        if hover.bounds != bounds || hover.text != self.text || !window.is_window_active() {
            hover.clear();
        }
        hover.bounds = bounds;
        hover.text = self.text.clone();
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        hover.hitbox = Some(hitbox.clone());
        if let Some(view) = hover.visible.clone() {
            let weak = Rc::downgrade(&state);
            window.set_tooltip(AnyTooltip {
                view,
                mouse_position: hover.anchor,
                check_visible_and_update: Rc::new(move |_, window, _| {
                    let Some(state) = weak.upgrade() else {
                        return false;
                    };
                    let mut hover = state.borrow_mut();
                    if !window.is_window_active()
                        || !hover.bounds.contains(&window.mouse_position())
                    {
                        hover.clear();
                    }
                    hover.visible.is_some()
                }),
            });
        }
        drop(hover);
        (state, hitbox)
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        (state, hitbox): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner.paint(window, cx);
        if !window.is_window_active() || !hitbox.is_hovered(window) {
            let mut hover = state.borrow_mut();
            let visible = hover.visible.is_some();
            hover.clear();
            hover.suppressed = false;
            if visible {
                window.defer(cx, |window, _| window.refresh());
            }
        }
        window.on_mouse_event({
            let state = state.clone();
            let hitbox = hitbox.clone();
            move |event: &MouseMoveEvent, phase, window, cx| {
                if !phase.capture() {
                    return;
                }
                let hovered = window.is_window_active() && hitbox.is_hovered(window);
                let mut hover = state.borrow_mut();
                if !hovered || event.pressed_button.is_some() || cx.has_active_drag() {
                    let visible = hover.visible.is_some();
                    hover.clear();
                    hover.suppressed = hovered;
                    if visible {
                        window.refresh();
                    }
                } else if !hover.suppressed && hover.pending.is_none() && hover.visible.is_none() {
                    let weak = Rc::downgrade(&state);
                    hover.pending = Some(window.spawn(cx, async move |cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(500))
                            .await;
                        let Some(state) = weak.upgrade() else {
                            return;
                        };
                        let _ = cx.update(|window, cx| {
                            let mut hover = state.borrow_mut();
                            if !hover.suppressed
                                && window.is_window_active()
                                && hover
                                    .hitbox
                                    .as_ref()
                                    .is_some_and(|hitbox| hitbox.is_hovered(window))
                                && hover.bounds.contains(&window.mouse_position())
                            {
                                let text = hover.text.clone();
                                hover.visible = Some(
                                    Tooltip::element(move |_, _| {
                                        div().child(text.clone()).map(|view| {
                                            #[cfg(test)]
                                            let view = view.on_children_prepainted(|_, _, cx| {
                                                cx.set_global(tests::Painted(true))
                                            });
                                            view
                                        })
                                    })
                                    .build(window, cx),
                                );
                                hover.anchor = window.mouse_position();
                                window.refresh();
                            }
                            hover.pending = None;
                        });
                    }));
                }
            }
        });
        // Capture runs even when a button or menu stops propagation. Suppress
        // reappearance under a stationary pointer until it leaves the control.
        window.on_mouse_event({
            let state = state.clone();
            move |_: &MouseDownEvent, phase, window, _| {
                if phase.capture() {
                    dismiss(&state, window);
                }
            }
        });
        window.on_mouse_event({
            let state = state.clone();
            move |_: &ScrollWheelEvent, phase, window, _| {
                if phase.capture() {
                    dismiss(&state, window);
                }
            }
        });
    }
}
fn dismiss(state: &RefCell<HoverState>, window: &mut Window) {
    let mut hover = state.borrow_mut();
    let visible = hover.visible.is_some();
    hover.clear();
    hover.suppressed = hover.bounds.contains(&window.mouse_position());
    if visible {
        window.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::HoverTooltipExt;
    use gpui::{
        AppContext, Context, Entity, Modifiers, Render, ScrollDelta, ScrollWheelEvent,
        TestAppContext, TouchPhase, VisualTestContext, Window, div, point, prelude::*, px,
    };
    use gpui_component::Root;
    use gpui_component::button::Button;
    use std::time::Duration;

    pub(super) struct Painted(pub bool);
    impl gpui::Global for Painted {}

    fn visible(visual: &mut VisualTestContext) -> bool {
        visual.update(|_, cx| cx.global::<Painted>().0)
    }

    struct Fixture {
        x: f32,
        shown: bool,
        covered: bool,
        label: &'static str,
        clicks: usize,
    }
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .relative()
                .child(
                    gpui::canvas(|_, _, cx| cx.set_global(Painted(false)), |_, _, _, _| {})
                        .absolute()
                        .inset_0(),
                )
                .when(self.shown, |view| {
                    view.child(
                        Button::new("button")
                            .label("Details")
                            .absolute()
                            .left(px(self.x))
                            .top(px(20.))
                            .w(px(100.))
                            .h(px(32.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.clicks += 1;
                                cx.notify();
                            }))
                            .hover_tooltip("hint", self.label),
                    )
                })
                .when(self.covered, |view| {
                    view.child(div().absolute().inset_0().occlude())
                })
        }
    }
    fn setup(cx: &mut TestAppContext) -> (Entity<Fixture>, VisualTestContext) {
        cx.update(gpui_component::init);
        let mut fixture = None;
        let window = cx.add_window(|window, cx| {
            window.activate_window();
            let view = cx.new(|_| Fixture {
                x: 20.,
                shown: true,
                covered: false,
                label: "Toggle details",
                clicks: 0,
            });
            fixture = Some(view.clone());
            Root::new(view, window, cx)
        });
        cx.run_until_parked();
        (
            fixture.unwrap(),
            VisualTestContext::from_window(*window, cx),
        )
    }
    fn hover(visual: &mut VisualTestContext, cx: &mut TestAppContext, x: f32) {
        visual.simulate_mouse_move(point(px(300.), px(200.)), None, Modifiers::none());
        visual.simulate_mouse_move(point(px(x), px(30.)), None, Modifiers::none());
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        assert!(visible(visual), "a normal hover must show its hint");
    }
    fn change(view: &Entity<Fixture>, cx: &mut TestAppContext, f: impl FnOnce(&mut Fixture)) {
        view.update(cx, |view, cx| {
            f(view);
            cx.notify();
        });
        cx.run_until_parked();
    }

    #[gpui::test]
    fn hints_close_on_leave_click_scroll_and_blur(cx: &mut TestAppContext) {
        let (view, mut visual) = setup(cx);
        hover(&mut visual, cx, 40.);
        visual.simulate_mouse_move(point(px(300.), px(200.)), None, Modifiers::none());
        assert!(!visible(&mut visual));
        hover(&mut visual, cx, 40.);
        visual.simulate_click(point(px(40.), px(30.)), Modifiers::none());
        assert_eq!(
            view.read_with(cx, |view, _| view.clicks),
            1,
            "tooltip must not intercept the button action"
        );
        visual.simulate_mouse_move(point(px(41.), px(30.)), None, Modifiers::none());
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        assert!(
            !visible(&mut visual),
            "click must suppress another hint until pointer exit"
        );
        hover(&mut visual, cx, 40.);
        visual.simulate_event(ScrollWheelEvent {
            position: point(px(40.), px(30.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-20.))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        assert!(!visible(&mut visual));
        hover(&mut visual, cx, 40.);
        visual.simulate_keystrokes("escape");
        assert!(!visible(&mut visual));
        // Clicking before the show delay expires must cancel the timer too.
        visual.simulate_mouse_move(point(px(300.), px(200.)), None, Modifiers::none());
        visual.simulate_mouse_move(point(px(40.), px(30.)), None, Modifiers::none());
        visual.simulate_click(point(px(40.), px(30.)), Modifiers::none());
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        assert!(!visible(&mut visual));
        hover(&mut visual, cx, 40.);
        visual.deactivate_window();
        assert!(!visible(&mut visual));
    }

    #[gpui::test]
    fn layout_changes_hide_visible_and_cancel_pending_hints(cx: &mut TestAppContext) {
        let (view, mut visual) = setup(cx);
        hover(&mut visual, cx, 40.);
        change(&view, cx, |view| view.x = 200.);
        assert!(
            !visible(&mut visual),
            "moving an animated button must invalidate its old hover bounds"
        );
        hover(&mut visual, cx, 220.);
        change(&view, cx, |view| view.shown = false);
        assert!(!visible(&mut visual));
        change(&view, cx, |view| {
            view.shown = true;
            view.x = 20.;
        });
        visual.simulate_mouse_move(point(px(40.), px(30.)), None, Modifiers::none());
        change(&view, cx, |view| view.x = 200.);
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        assert!(
            !visible(&mut visual),
            "a timer started at the old position must not show a hint later"
        );
        hover(&mut visual, cx, 220.);
        change(&view, cx, |view| view.label = "New action");
        assert!(
            !visible(&mut visual),
            "a reused control must not keep the previous action's hint"
        );
    }

    #[gpui::test]
    fn covered_controls_and_removed_pending_controls_do_not_leave_hints(cx: &mut TestAppContext) {
        let (view, mut visual) = setup(cx);
        hover(&mut visual, cx, 40.);
        change(&view, cx, |view| view.covered = true);
        assert!(!visible(&mut visual));
        change(&view, cx, |view| view.covered = false);
        visual.simulate_mouse_move(point(px(300.), px(200.)), None, Modifiers::none());
        visual.simulate_mouse_move(point(px(40.), px(30.)), None, Modifiers::none());
        change(&view, cx, |view| view.shown = false);
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        assert!(!visible(&mut visual));
        change(&view, cx, |view| view.shown = true);
        assert!(
            !visible(&mut visual),
            "a removed timer must not resurrect after remount"
        );
        hover(&mut visual, cx, 40.);
    }
}
