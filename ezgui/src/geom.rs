use crate::{svg, Color, Drawable, EventCtx, FancyColor, GfxCtx, Prerender, ScreenDims};
use geom::{Angle, Bounds, Polygon, Pt2D};

/// A mutable builder for a group of colored polygons.
#[derive(Clone)]
pub struct GeomBatch {
    pub(crate) list: Vec<(FancyColor, Polygon)>,
    pub autocrop_dims: bool,
}

impl GeomBatch {
    /// Creates an empty batch.
    pub fn new() -> GeomBatch {
        GeomBatch {
            list: Vec::new(),
            autocrop_dims: true,
        }
    }

    /// Creates a batch of colored polygons.
    pub fn from(list: Vec<(Color, Polygon)>) -> GeomBatch {
        GeomBatch {
            list: list
                .into_iter()
                .map(|(c, p)| (FancyColor::RGBA(c), p))
                .collect(),
            autocrop_dims: true,
        }
    }

    /// Adds a single colored polygon.
    pub fn push(&mut self, color: Color, p: Polygon) {
        self.list.push((FancyColor::RGBA(color), p));
    }
    // TODO Weird API too
    pub fn fancy_push(&mut self, color: FancyColor, p: Polygon) {
        self.list.push((color, p));
    }

    /// Applies one color to many polygons.
    pub fn extend(&mut self, color: Color, polys: Vec<Polygon>) {
        for p in polys {
            self.list.push((FancyColor::RGBA(color), p));
        }
    }

    /// Appends all colored polygons from another batch to the current one.
    pub fn append(&mut self, other: GeomBatch) {
        self.list.extend(other.list);
    }

    /// Returns the colored polygons in this batch, destroying the batch.
    pub fn consume(self) -> Vec<(FancyColor, Polygon)> {
        self.list
    }

    /// Draws the batch, consuming it. Only use this for drawing things once.
    pub fn draw(self, g: &mut GfxCtx) {
        let refs = self
            .list
            .iter()
            .map(|(color, p)| (color.clone(), p))
            .collect();
        let obj = g.prerender.upload_temporary(refs);
        g.redraw(&obj);
    }

    /// Upload the batch of polygons to the GPU, returning something that can be cheaply redrawn
    /// many times later.
    pub fn upload(self, ctx: &EventCtx) -> Drawable {
        ctx.prerender.upload(self)
    }

    /// Sets the top-left to 0, 0. Not sure exactly when this should be used.
    pub fn autocrop(mut self) -> GeomBatch {
        let mut bounds = Bounds::new();
        for (_, poly) in &self.list {
            bounds.union(poly.get_bounds());
        }
        if bounds.min_x == 0.0 && bounds.min_y == 0.0 {
            return self;
        }
        for (_, poly) in &mut self.list {
            *poly = poly.translate(-bounds.min_x, -bounds.min_y);
        }
        self
    }

    /// Builds a single polygon covering everything in this batch. Use to create a hitbox.
    pub fn unioned_polygon(&self) -> Polygon {
        let mut result = self.list[0].1.clone();
        for (_, p) in &self.list[1..] {
            result = result.union(p.clone());
        }
        result
    }

    /// True when the batch is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Returns the width and height of all geometry contained in the batch.
    pub fn get_dims(&self) -> ScreenDims {
        // TODO Maybe warn about this happening and avoid in the first place? Sometimes we wind up
        // trying to draw completely empty text.
        if self.is_empty() {
            return ScreenDims::new(0.0, 0.0);
        }
        let mut bounds = Bounds::new();
        for (_, poly) in &self.list {
            bounds.union(poly.get_bounds());
        }
        if self.autocrop_dims {
            ScreenDims::new(bounds.width(), bounds.height())
        } else {
            ScreenDims::new(bounds.max_x, bounds.max_y)
        }
    }

    // TODO Weird API.
    /// Adds an SVG image to the current batch, applying the transformations first.
    pub fn add_svg(
        &mut self,
        prerender: &Prerender,
        filename: &str,
        center: Pt2D,
        scale: f64,
        rotate: Angle,
        rewrite: RewriteColor,
        map_space: bool,
    ) {
        self.append(
            svg::load_svg(
                prerender,
                filename,
                if map_space {
                    1.0
                } else {
                    *prerender.assets.scale_factor.borrow()
                },
            )
            .0
            .scale(scale)
            .centered_on(center)
            .rotate(rotate)
            .color(rewrite),
        );
    }

    // TODO Weird API
    /// Parse an SVG string and add it to the batch.
    pub fn add_svg_contents(&mut self, raw: Vec<u8>) {
        let svg_tree = usvg::Tree::from_data(&raw, &usvg::Options::default()).unwrap();
        svg::add_svg_inner(self, svg_tree, svg::HIGH_QUALITY, 1.0).unwrap();
    }

    // TODO Weird API
    /// Adds geometry from another batch to the current batch, first centering it on the given
    /// point.
    pub fn add_centered(&mut self, other: GeomBatch, center: Pt2D) {
        self.append(other.centered_on(center));
    }

    /// Transforms all colors in a batch.
    pub fn color(mut self, transformation: RewriteColor) -> GeomBatch {
        for (fancy, _) in &mut self.list {
            if let FancyColor::RGBA(ref mut c) = fancy {
                *c = transformation.apply(*c);
            }
        }
        self
    }

    /// Translates the batch to be centered on some point.
    pub fn centered_on(self, center: Pt2D) -> GeomBatch {
        let dims = self.get_dims();
        let dx = center.x() - dims.width / 2.0;
        let dy = center.y() - dims.height / 2.0;
        self.translate(dx, dy)
    }

    /// Translates the batch by some offset.
    pub fn translate(mut self, dx: f64, dy: f64) -> GeomBatch {
        for (_, poly) in &mut self.list {
            *poly = poly.translate(dx, dy);
        }
        self
    }

    /// Rotates each polygon in the batch relative to the center of that polygon.
    pub fn rotate(mut self, angle: Angle) -> GeomBatch {
        for (_, poly) in &mut self.list {
            *poly = poly.rotate(angle);
        }
        self
    }

    /// Scales the batch by some factor.
    pub fn scale(mut self, factor: f64) -> GeomBatch {
        for (_, poly) in &mut self.list {
            *poly = poly.scale(factor);
        }
        self
    }
}

pub enum RewriteColor {
    NoOp,
    Change(Color, Color),
    ChangeMore(Vec<(Color, Color)>),
    ChangeAll(Color),
    ChangeAlpha(f32),
}

impl RewriteColor {
    fn apply(&self, c: Color) -> Color {
        match self {
            RewriteColor::NoOp => c,
            RewriteColor::Change(from, to) => {
                if c == *from {
                    *to
                } else {
                    c
                }
            }
            RewriteColor::ChangeMore(ref list) => {
                for (from, to) in list {
                    if c == *from {
                        return *to;
                    }
                }
                c
            }
            RewriteColor::ChangeAll(to) => *to,
            RewriteColor::ChangeAlpha(alpha) => c.alpha(*alpha),
        }
    }
}
