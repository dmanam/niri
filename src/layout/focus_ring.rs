use std::iter::zip;

use niri_config::{CornerRadius, Gradient, GradientRelativeTo};
use smithay::backend::renderer::element::{Element as _, Kind};
use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::niri_render_elements;
use crate::render_helpers::border::BorderRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};

#[derive(Debug)]
pub struct FocusRing {
    buffers: [SolidColorBuffer; 8],
    locations: [Point<f64, Logical>; 8],
    sizes: [Size<f64, Logical>; 8],
    borders: [BorderRenderElement; 8],
    full_size: Size<f64, Logical>,
    is_border: bool,
    use_border_shader: bool,
    config: niri_config::FocusRing,
    thicken_corners: bool,
    slack: Size<f64, Logical>,
}

niri_render_elements! {
    FocusRingRenderElement => {
        SolidColor = SolidColorRenderElement,
        Gradient = BorderRenderElement,
    }
}

impl FocusRing {
    pub fn new(config: niri_config::FocusRing) -> Self {
        Self {
            buffers: Default::default(),
            locations: Default::default(),
            sizes: Default::default(),
            borders: Default::default(),
            full_size: Default::default(),
            is_border: false,
            use_border_shader: false,
            config,
            thicken_corners: true,
            slack: Size::default(),
        }
    }

    pub fn update_config(&mut self, config: niri_config::FocusRing) {
        self.config = config;
    }

    pub fn update_shaders(&mut self) {
        for elem in &mut self.borders {
            elem.damage_all();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_render_elements(
        &mut self,
        win_size: Size<f64, Logical>,
        is_active: bool,
        is_border: bool,
        is_urgent: bool,
        view_rect: Rectangle<f64, Logical>,
        radius: CornerRadius,
        scale: f64,
        alpha: f32,
    ) {
        let width = self.config.width;
        self.full_size = win_size + Size::from((width, width)).upscale(2.);
        self.is_border = is_border;

        let color = if is_urgent {
            self.config.urgent_color
        } else if is_active {
            self.config.active_color
        } else {
            self.config.inactive_color
        };

        for buf in &mut self.buffers {
            buf.set_color(color);
        }

        let radius = radius.fit_to(self.full_size.w as f32, self.full_size.h as f32);

        let gradient = if is_urgent {
            self.config.urgent_gradient
        } else if is_active {
            self.config.active_gradient
        } else {
            self.config.inactive_gradient
        };

        self.use_border_shader = radius != CornerRadius::default() || gradient.is_some();

        // Set the defaults for solid color + rounded corners.
        let gradient = gradient.unwrap_or_else(|| Gradient::from(color));

        let full_rect = Rectangle::new(Point::from((-width, -width)), self.full_size);
        let gradient_area = match gradient.relative_to {
            GradientRelativeTo::Window => full_rect,
            GradientRelativeTo::WorkspaceView => view_rect,
        };

        let rounded_corner_border_width = if is_border {
            // HACK: increase the border width used for the inner rounded corners a tiny bit to
            // reduce background bleed.
            let extra = if self.thicken_corners { 0.5 } else { 0. };
            width as f32 + extra
        } else {
            0.
        };

        let ceil = |logical: f64| (logical * scale).ceil() / scale;

        // All of this stuff should end up aligned to physical pixels because:
        // * Window size and border width are rounded to physical pixels before being passed to this
        //   function.
        // * We will ceil the corner radii below.
        // * We do not divide anything, only add, subtract and multiply by integers.
        // * At rendering time, tile positions are rounded to physical pixels.
        //
        // The exception is the slack: the sub-pixel remainder between the tile size and the size
        // the layout allocated for the tile. The pieces at the right and bottom stretch by the
        // slack so that the border outer edge reaches the allocated boundary, where the adjacent
        // tile or the screen edge is. The stretched sizes round to physical pixels the same way
        // as the adjacent tile positions do, so the pieces still meet them exactly.
        let slack = self.slack;

        if is_border {
            let top_left = f64::max(width, ceil(f64::from(radius.top_left)));
            let top_right = f64::min(
                self.full_size.w - top_left,
                f64::max(width, ceil(f64::from(radius.top_right))),
            );
            let bottom_left = f64::min(
                self.full_size.h - top_left,
                f64::max(width, ceil(f64::from(radius.bottom_left))),
            );
            let bottom_right = f64::min(
                self.full_size.h - top_right,
                f64::min(
                    self.full_size.w - bottom_left,
                    f64::max(width, ceil(f64::from(radius.bottom_right))),
                ),
            );

            // Top edge.
            self.sizes[0] = Size::from((win_size.w + width * 2. - top_left - top_right, width));
            self.locations[0] = Point::from((-width + top_left, -width));

            // Bottom edge.
            self.sizes[1] = Size::from((
                win_size.w + width * 2. - bottom_left - bottom_right,
                width + slack.h,
            ));
            self.locations[1] = Point::from((-width + bottom_left, win_size.h));

            // Left edge.
            self.sizes[2] = Size::from((width, win_size.h + width * 2. - top_left - bottom_left));
            self.locations[2] = Point::from((-width, -width + top_left));

            // Right edge.
            self.sizes[3] = Size::from((
                width + slack.w,
                win_size.h + width * 2. - top_right - bottom_right,
            ));
            self.locations[3] = Point::from((win_size.w, -width + top_right));

            // Top-left corner.
            self.sizes[4] = Size::from((top_left, top_left));
            self.locations[4] = Point::from((-width, -width));

            // Top-right corner.
            self.sizes[5] = Size::from((top_right + slack.w, top_right));
            self.locations[5] = Point::from((win_size.w + width - top_right, -width));

            // Bottom-right corner.
            self.sizes[6] = Size::from((bottom_right + slack.w, bottom_right + slack.h));
            self.locations[6] = Point::from((
                win_size.w + width - bottom_right,
                win_size.h + width - bottom_right,
            ));

            // Bottom-left corner.
            self.sizes[7] = Size::from((bottom_left, bottom_left + slack.h));
            self.locations[7] = Point::from((-width, win_size.h + width - bottom_left));

            for (buf, size) in zip(&mut self.buffers, self.sizes) {
                buf.resize(size);
            }

            for (border, (loc, size)) in zip(&mut self.borders, zip(self.locations, self.sizes)) {
                border.update(
                    size,
                    Rectangle::new(gradient_area.loc - loc, gradient_area.size),
                    gradient.in_,
                    gradient.from,
                    gradient.to,
                    ((gradient.angle as f32) - 90.).to_radians(),
                    Rectangle::new(full_rect.loc - loc, full_rect.size),
                    rounded_corner_border_width,
                    radius,
                    scale as f32,
                    alpha,
                );
            }
        } else {
            self.sizes[0] = self.full_size + slack;
            self.buffers[0].resize(self.sizes[0]);
            self.locations[0] = Point::from((-width, -width));

            self.borders[0].update(
                self.sizes[0],
                Rectangle::new(gradient_area.loc - self.locations[0], gradient_area.size),
                gradient.in_,
                gradient.from,
                gradient.to,
                ((gradient.angle as f32) - 90.).to_radians(),
                Rectangle::new(full_rect.loc - self.locations[0], full_rect.size),
                rounded_corner_border_width,
                radius,
                scale as f32,
                alpha,
            );
        }
    }

    pub fn render(
        &self,
        renderer: &mut impl NiriRenderer,
        location: Point<f64, Logical>,
        push: &mut dyn FnMut(FocusRingRenderElement),
    ) {
        if self.config.off {
            return;
        }

        let border_width = -self.locations[0].y;

        // If drawing as a border with width = 0, then there's nothing to draw.
        if self.is_border && border_width == 0. {
            return;
        }

        let has_border_shader = BorderRenderElement::has_shader(renderer);

        let mut push = |buffer, border: &BorderRenderElement, location: Point<f64, Logical>| {
            let elem = if self.use_border_shader && has_border_shader {
                border.clone().with_location(location).into()
            } else {
                let alpha = border.alpha();
                SolidColorRenderElement::from_buffer(buffer, location, alpha, Kind::Unspecified)
                    .into()
            };
            push(elem);
        };

        if self.is_border {
            for ((buf, border), loc) in zip(zip(&self.buffers, &self.borders), self.locations) {
                push(buf, border, location + loc);
            }
        } else {
            push(
                &self.buffers[0],
                &self.borders[0],
                location + self.locations[0],
            );
        }
    }

    pub fn width(&self) -> f64 {
        self.config.width
    }

    pub fn is_off(&self) -> bool {
        self.config.off
    }

    pub fn set_thicken_corners(&mut self, value: bool) {
        self.thicken_corners = value;
    }

    /// Sets the sub-pixel slack that the right and bottom of the border should stretch by.
    ///
    /// Takes effect upon the next [`FocusRing::update_render_elements()`].
    ///
    /// See `Tile::compute_border_slack()` for how the slack is derived.
    pub fn set_slack(&mut self, slack: Size<f64, Logical>) {
        self.slack = slack;
    }

    #[cfg(test)]
    pub fn slack(&self) -> Size<f64, Logical> {
        self.slack
    }

    pub fn config(&self) -> &niri_config::FocusRing {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn border(width: f64) -> FocusRing {
        FocusRing::new(niri_config::FocusRing {
            off: false,
            width,
            ..Default::default()
        })
    }

    #[test]
    fn slack_stretches_right_and_bottom_border_pieces() {
        let width = 2.;
        let mut ring = border(width);
        let win_size = Size::from((100., 50.));
        let slack = Size::from((0.5, 0.25));
        ring.set_slack(slack);
        ring.update_render_elements(
            win_size,
            false,
            true, // is_border
            false,
            Rectangle::from_size(win_size),
            CornerRadius::default(),
            1.,
            1.,
        );

        // Right edge and right-side corners stretch horizontally by slack.w; bottom edge and
        // bottom-side corners stretch vertically by slack.h. The left/top pieces are unchanged.
        assert_eq!(ring.sizes[3].w, width + slack.w, "right edge width");
        assert_eq!(ring.sizes[5].w, width + slack.w, "top-right corner width");
        assert_eq!(ring.sizes[6].w, width + slack.w, "bottom-right corner width");
        assert_eq!(ring.sizes[1].h, width + slack.h, "bottom edge height");
        assert_eq!(ring.sizes[6].h, width + slack.h, "bottom-right corner height");
        assert_eq!(ring.sizes[7].h, width + slack.h, "bottom-left corner height");

        // The far edges of the border now reach win_size + border + slack, i.e. the boundary the
        // layout allocated for the tile.
        let right = ring.locations[3].x + ring.sizes[3].w;
        assert_eq!(right, win_size.w + width + slack.w);
        let bottom = ring.locations[1].y + ring.sizes[1].h;
        assert_eq!(bottom, win_size.h + width + slack.h);
    }

    #[test]
    fn zero_slack_leaves_border_pieces_unchanged() {
        let width = 2.;
        let mut ring = border(width);
        let win_size = Size::from((100., 50.));
        ring.set_slack(Size::default());
        ring.update_render_elements(
            win_size,
            false,
            true,
            false,
            Rectangle::from_size(win_size),
            CornerRadius::default(),
            1.,
            1.,
        );

        let right = ring.locations[3].x + ring.sizes[3].w;
        assert_eq!(right, win_size.w + width);
        let bottom = ring.locations[1].y + ring.sizes[1].h;
        assert_eq!(bottom, win_size.h + width);
    }
}
