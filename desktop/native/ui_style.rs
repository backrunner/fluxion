//! Shared presentation for the native workspace. Keep geometry independent of
//! the rem scale so localized controls retain the same comfortable hit targets.
use gpui::{prelude::*, *};
use gpui_component::{button::*, *};

const SCROLL_FADE_DEPTH: f32 = 14.;

fn scroll_fade_strength(offset: f32, max_offset: f32) -> (f32, f32) {
    let max_offset = max_offset.max(0.);
    let position = (-offset).clamp(0., max_offset);
    (
        (position / SCROLL_FADE_DEPTH).min(1.),
        ((max_offset - position) / SCROLL_FADE_DEPTH).min(1.),
    )
}

/// Paint above the scroll content using this frame's measured scroll bounds.
/// Canvas adds no hitbox, so edge controls and nested scrolling remain usable.
pub fn scroll_mask(content: impl IntoElement, scroll: &ScrollHandle, background: Hsla) -> Div {
    let scroll = scroll.clone();
    div()
        .relative()
        .w_full()
        .min_h_0()
        .overflow_hidden()
        .child(content)
        .child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let bounds = scroll.bounds();
                    let depth = px(SCROLL_FADE_DEPTH).min(bounds.size.height / 2.);
                    let (top, bottom) = scroll_fade_strength(
                        scroll.offset().y.into(),
                        scroll.max_offset().height.into(),
                    );
                    for (strength, y, angle) in [
                        (top, bounds.top(), 180.),
                        (bottom, bounds.bottom() - depth, 0.),
                    ] {
                        if strength <= 0. || depth <= px(0.) {
                            continue;
                        }
                        window.paint_quad(PaintQuad {
                            bounds: Bounds::new(
                                point(bounds.left(), y),
                                size(bounds.size.width, depth),
                            ),
                            background: linear_gradient(
                                angle,
                                LinearColorStop {
                                    color: background.opacity(strength),
                                    percentage: 0.,
                                },
                                LinearColorStop {
                                    color: background.opacity(0.),
                                    percentage: 1.,
                                },
                            ),
                            corner_radii: Corners::all(px(0.)),
                            border_widths: Edges::all(px(0.)),
                            border_color: transparent_black(),
                            border_style: BorderStyle::default(),
                        });
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
}

pub fn control(id: impl Into<ElementId>) -> Button {
    Button::new(id).small().h(px(30.)).px_3().rounded(px(6.))
}

pub fn separator(cx: &App) -> Div {
    div()
        .w_full()
        .h(px(1.))
        .flex_shrink_0()
        .bg(cx.theme().border.opacity(0.7))
}

pub fn settings_group(title: String, content: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .gap_2()
        .child(div().px_1().text_size(px(13.)).font_semibold().child(title))
        .child(
            v_flex()
                .w_full()
                .p_4()
                .gap_3()
                .rounded(px(10.))
                .border_1()
                .border_color(cx.theme().border.opacity(0.7))
                .bg(cx.theme().secondary.opacity(0.45))
                .child(content),
        )
}

pub fn preference_row(label: String, control: impl IntoElement) -> Div {
    h_flex()
        .w_full()
        .min_h(px(32.))
        .gap_3()
        .flex_wrap()
        .child(
            div()
                .flex_1()
                .min_w(px(100.))
                .text_size(px(13.))
                .child(label),
        )
        .child(control)
}

pub fn segments(cx: &App) -> Div {
    h_flex()
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(7.))
        .bg(cx.theme().secondary_hover.opacity(0.65))
}

pub fn segment(id: impl Into<ElementId>, label: String, selected: bool, cx: &App) -> Button {
    control(id)
        .ghost()
        .h(px(26.))
        .px_2()
        .child(div().text_size(px(13.)).child(label))
        .when(selected, |button| {
            button
                .bg(if cx.theme().is_dark() {
                    cx.theme().secondary_hover
                } else {
                    cx.theme().background
                })
                .border_color(cx.theme().border.opacity(0.6))
                .text_color(cx.theme().foreground)
                .shadow_xs()
        })
}

/// Presentation state for a disclosure card. Keep the last measured natural
/// height so closing and interrupted transitions never jump to a guessed size.
pub struct CollapseState {
    pub open: bool,
    from: f32,
    started: Option<std::time::Instant>,
    height: std::rc::Rc<std::cell::Cell<Pixels>>,
}

impl Default for CollapseState {
    fn default() -> Self {
        Self {
            open: false,
            from: 0.,
            started: None,
            height: std::rc::Rc::new(std::cell::Cell::new(px(0.))),
        }
    }
}

impl CollapseState {
    fn progress(&self) -> f32 {
        let target = if self.open { 1. } else { 0. };
        let Some(started) = self.started else {
            return target;
        };
        let elapsed = (started.elapsed().as_secs_f32() / 0.22).min(1.);
        let eased = 1. - (1. - elapsed).powi(3);
        self.from + (target - self.from) * eased
    }

    pub fn toggle(&mut self) {
        self.set_open(!self.open);
    }

    pub fn set_open(&mut self, open: bool) {
        if self.open == open {
            return;
        }
        self.from = self.progress();
        self.open = open;
        self.started = Some(std::time::Instant::now());
    }

    pub fn frame(&mut self, window: &mut Window) -> f32 {
        let progress = self.progress();
        if self
            .started
            .is_some_and(|started| started.elapsed().as_secs_f32() < 0.22)
        {
            window.request_animation_frame();
        } else {
            self.started = None;
        }
        progress
    }
}

/// A full-width disclosure header and clipped, naturally sized form content.
pub fn collapse_card(
    state: &mut CollapseState,
    header: Button,
    action: Option<Button>,
    content: Div,
    window: &mut Window,
    cx: &App,
) -> Div {
    let progress = state.progress();
    let animating = state
        .started
        .is_some_and(|started| started.elapsed().as_secs_f32() < 0.22);
    if animating {
        window.request_animation_frame();
    } else {
        state.started = None;
    }
    let mut card = v_flex()
        .w_full()
        .flex_shrink_0()
        .rounded(px(10.))
        .border_1()
        .border_color(cx.theme().border.opacity(0.7))
        .bg(cx.theme().secondary.opacity(0.35))
        .overflow_hidden()
        .child(
            h_flex()
                .w_full()
                .hover(|row| row.bg(cx.theme().foreground.opacity(0.025)))
                .child(
                    header
                        .custom(
                            ButtonCustomVariant::new(cx)
                                .foreground(cx.theme().secondary_foreground)
                                .active(cx.theme().foreground.opacity(0.05)),
                        )
                        .flex_1()
                        .min_w_0()
                        .rounded(px(0.))
                        .child(
                            Icon::new(IconName::ChevronRight)
                                .xsmall()
                                .text_color(cx.theme().muted_foreground)
                                .transform(Transformation::rotate(percentage(progress * 0.25))),
                        ),
                )
                .when_some(action, |row, button| {
                    row.child(div().pr_2().flex_shrink_0().child(button))
                }),
        );
    if state.open || animating {
        let height = state.height.clone();
        card = card.child(
            div()
                .w_full()
                .flex_shrink_0()
                .overflow_hidden()
                .when(animating, |view| view.h(state.height.get() * progress))
                .on_children_prepainted(move |bounds, _, _| {
                    if let Some(bounds) = bounds.first() {
                        height.set(bounds.size.height);
                    }
                })
                .child(
                    content
                        .w_full()
                        .flex_shrink_0()
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border.opacity(0.6)),
                ),
        );
    }
    card
}

#[cfg(test)]
mod scroll_mask_tests {
    use super::scroll_fade_strength;

    #[test]
    fn fades_only_on_edges_with_hidden_content() {
        assert_eq!(scroll_fade_strength(0., 0.), (0., 0.));
        assert_eq!(scroll_fade_strength(0., 200.), (0., 1.));
        assert_eq!(scroll_fade_strength(-100., 200.), (1., 1.));
        assert_eq!(scroll_fade_strength(-200., 200.), (1., 0.));
        // Overscroll and content shrinking must not leave a stale edge fade.
        assert_eq!(scroll_fade_strength(10., 200.), (0., 1.));
        assert_eq!(scroll_fade_strength(-220., 200.), (1., 0.));
        assert_eq!(scroll_fade_strength(-200., 0.), (0., 0.));
    }

    #[test]
    fn fades_gently_near_scroll_boundaries() {
        assert_eq!(scroll_fade_strength(-7., 200.), (0.5, 1.));
        assert_eq!(scroll_fade_strength(-193., 200.), (1., 0.5));
    }
}
