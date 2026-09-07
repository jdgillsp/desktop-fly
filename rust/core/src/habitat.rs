//! An optional enclosure: a bounded patch of desktop the creature stays inside,
//! with a few props it notices and pushes around.
//!
//! Two ideas, kept separate on purpose:
//!
//! - [`Region`] is *where the world ends*. Every body already turned away from
//!   the edge of the display and clamped itself inside it — but each did so
//!   against `±bounds/2`, which silently assumed the region was centred on the
//!   scene origin. It always was, so the assumption was invisible and free. A
//!   habitat parked in a corner breaks it, so `Region` carries a centre and the
//!   bodies ask it questions instead of doing the arithmetic themselves. With
//!   [`Region::centered`] the answers are bit-identical to the old ones, which
//!   is why free-roam is untouched by this whole feature.
//! - [`Habitat`] is *the enclosure itself*: which kind of tank, and the props
//!   in it. It is pure state and a `step`; the geometry lives in the shell,
//!   like every other body's does.
//!
//! ## Why the props cannot be clicked
//!
//! The overlay is click-through by contract — `WS_EX_TRANSPARENT`, plus
//! `set_cursor_hittest(false)`, and the README promises it never intercepts the
//! user's mouse. A prop you could click would be a hole in the desktop, and the
//! moment there is one hole there is a reason to add another. So interaction
//! runs the other way round: props react to the **creature**, and to the
//! cursor's *proximity* — the same shadow-of-a-hand channel the creatures
//! already sense through. Nothing in here consumes a click.

use crate::creature::Substrate;
use crate::rng::Pcg32;
use crate::util::{clamp, hypot, Vec2};

/// An axis-aligned patch of scene the creature lives inside.
///
/// Scene space is centred on the display with y up (see `ScreenSpace`), so the
/// whole-display case is a `Region` at the origin and everything downstream is
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub center: Vec2,
    pub size: (f32, f32),
}

impl Region {
    /// The free-roam region: the whole display, centred on the origin. This is
    /// exactly what the bodies assumed before habitats existed.
    pub fn centered(size: (f32, f32)) -> Self {
        Region {
            center: Vec2::ZERO,
            size,
        }
    }

    pub fn new(center: Vec2, size: (f32, f32)) -> Self {
        Region { center, size }
    }

    #[inline]
    pub fn half(&self) -> (f32, f32) {
        (self.size.0 / 2.0, self.size.1 / 2.0)
    }

    #[inline]
    pub fn min(&self) -> Vec2 {
        Vec2::new(
            self.center.x - self.size.0 / 2.0,
            self.center.y - self.size.1 / 2.0,
        )
    }

    #[inline]
    pub fn max(&self) -> Vec2 {
        Vec2::new(
            self.center.x + self.size.0 / 2.0,
            self.center.y + self.size.1 / 2.0,
        )
    }

    /// Is `p` within `margin` of the wall, or past it?
    #[inline]
    pub fn outside(&self, p: Vec2, margin: f32) -> bool {
        let (hw, hh) = self.half();
        (p.x - self.center.x).abs() > hw - margin || (p.y - self.center.y).abs() > hh - margin
    }

    /// Pull `p` back inside, leaving `margin` of clearance. A margin wider than
    /// the region collapses to its centre rather than inverting the interval.
    #[inline]
    pub fn clamp_inside(&self, p: Vec2, margin: f32) -> Vec2 {
        let (hw, hh) = self.half();
        let (ex, ey) = ((hw - margin).max(0.0), (hh - margin).max(0.0));
        Vec2::new(
            clamp(p.x, self.center.x - ex, self.center.x + ex),
            clamp(p.y, self.center.y - ey, self.center.y + ey),
        )
    }

    /// A point drawn uniformly from the region, `margin` in from the wall.
    pub fn sample(&self, rng: &mut Pcg32, margin: f32) -> Vec2 {
        let (hw, hh) = self.half();
        let (ex, ey) = ((hw - margin).max(0.0), (hh - margin).max(0.0));
        Vec2::new(
            self.center.x + rng.range(-ex, ex),
            self.center.y + rng.range(-ey, ey),
        )
    }

    /// Heading, in radians, from `p` back toward the middle. Every body steers
    /// by this when it finds itself near a wall.
    #[inline]
    pub fn bearing_home(&self, p: Vec2) -> f32 {
        (self.center.y - p.y).atan2(self.center.x - p.x)
    }
}

/// Which enclosure a creature gets. Chosen from its [`Substrate`], which is
/// what that enum was declared for — until now nothing branched on it.
///
/// One enclosure per way of moving, and each is the container that animal is
/// actually kept in, not a generic box in a different colour: a fruit fly lives
/// in a mesh rearing cage over a dish of medium, a jumping spider in a tall
/// arboreal vivarium, *C. elegans* on an agar plate seeded with bacteria, and a
/// koi in a pond. The shell draws them; this only says which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HabitatKind {
    /// A framed mesh cage: a dish of cornmeal medium, a piece of fruit, a spent
    /// culture vial. For anything that flies — it needs the height.
    FlyCage,
    /// A tall acrylic vivarium with cross-ventilation: cork bark against the
    /// back wall, a twig to climb, a silk retreat in the top corner. For a
    /// jumper.
    Vivarium,
    /// A shallow dish of agar with a bacterial lawn spread on it. For a
    /// crawler, which is on the gel, not in a box.
    AgarPlate,
    /// A stone-rimmed pond: dark bottom, lily pads on the surface, floating
    /// pellets. For a swimmer.
    Pond,
    /// A low glass tank with a deep bed of sand, a hide, and a water dish:
    /// what a hognose is kept in. For a burrower — the sand is the point,
    /// because the animal spends much of its time under it. (The sandworm
    /// gets the same tank, at a scale the fiction would find insulting.)
    SandTerrarium,
}

impl HabitatKind {
    pub const ALL: [HabitatKind; 5] = [
        HabitatKind::FlyCage,
        HabitatKind::Vivarium,
        HabitatKind::AgarPlate,
        HabitatKind::Pond,
        HabitatKind::SandTerrarium,
    ];

    pub fn for_substrate(s: Substrate) -> Self {
        match s {
            Substrate::WalkerFlier => HabitatKind::FlyCage,
            Substrate::WalkerJumper | Substrate::WalkerWeaver => HabitatKind::Vivarium,
            Substrate::Crawler => HabitatKind::AgarPlate,
            Substrate::Swimmer => HabitatKind::Pond,
            Substrate::Burrower => HabitatKind::SandTerrarium,
        }
    }

    /// Whether the enclosure holds water — the one distinction the *physics*
    /// cares about: food floats on a pond and sinks nowhere else, and the
    /// creature can occupy a water column.
    pub fn is_wet(&self) -> bool {
        matches!(self, HabitatKind::Pond)
    }

    /// Key for the settings file; see `PropKind::slug`.
    pub fn slug(&self) -> &'static str {
        match self {
            HabitatKind::FlyCage => "flycage",
            HabitatKind::Vivarium => "vivarium",
            HabitatKind::AgarPlate => "agarplate",
            HabitatKind::Pond => "pond",
            HabitatKind::SandTerrarium => "terrarium",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            HabitatKind::FlyCage => "fly cage",
            HabitatKind::Vivarium => "vivarium",
            HabitatKind::AgarPlate => "agar plate",
            HabitatKind::Pond => "pond",
            HabitatKind::SandTerrarium => "sand terrarium",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    /// Does not move. Scenery, and a landmark.
    Pebble,
    /// Anchored; sways when something passes close.
    Plant,
    /// Drifts, is approached, is eaten, comes back somewhere else.
    Food,
    /// A dish of culture medium. Anchored; a standing target that is never used
    /// up.
    Dish,
    /// A piece of fruit on the cage floor. Anchored; a standing target. The
    /// `variant` picks banana or apple.
    Fruit,
    /// A spent culture vial lying on its side. Scenery.
    Vial,
    /// A slab of cork bark leaning on the back wall. Scenery.
    Bark,
    /// A twig rising from the floor toward the top of the tank. Scenery.
    Twig,
    /// A bacterial lawn spread on agar. Anchored; the standing target a worm
    /// crawls toward and grazes.
    Lawn,
    /// Floats on the surface, drifts, and is nudged by whatever passes under
    /// it. The `variant` decides whether it carries a flower.
    LilyPad,
    /// A large rounded river stone on the pond bottom. Scenery.
    Cobble,
    /// A half-round of cork bark lying on the sand: the hide. Anchored; the
    /// standing target a snake goes to and rests under.
    Hide,
}

impl PropKind {
    /// What may be put into each enclosure, and in what order the tray lists
    /// it. Deliberately not "any prop in any tank": cork bark in a pond is not
    /// a decorating choice, it is a mistake, and the whole point of one
    /// enclosure per animal is that its contents belong to that animal.
    pub fn catalogue(habitat: HabitatKind) -> &'static [PropKind] {
        match habitat {
            HabitatKind::FlyCage => &[
                PropKind::Dish,
                PropKind::Fruit,
                PropKind::Vial,
                PropKind::Pebble,
            ],
            HabitatKind::Vivarium => &[
                PropKind::Bark,
                PropKind::Twig,
                PropKind::Plant,
                PropKind::Pebble,
            ],
            HabitatKind::AgarPlate => &[PropKind::Lawn, PropKind::Pebble],
            HabitatKind::Pond => &[
                PropKind::LilyPad,
                PropKind::Cobble,
                PropKind::Plant,
                PropKind::Food,
            ],
            HabitatKind::SandTerrarium => &[
                PropKind::Hide,
                PropKind::Cobble,
                PropKind::Plant,
                PropKind::Pebble,
            ],
        }
    }

    /// The size one of these is made at, in touch-radius units. `stock` used to
    /// carry these inline; adding props at runtime needs the same numbers, and
    /// two lists of them would drift.
    pub fn default_radius(&self) -> f32 {
        match self {
            PropKind::Pebble => 10.0,
            PropKind::Plant => 14.0,
            PropKind::Food => 5.0,
            PropKind::Dish => 24.0,
            PropKind::Fruit => 15.0,
            PropKind::Vial => 11.0,
            PropKind::Bark => 28.0,
            PropKind::Twig => 18.0,
            // Overridden by the plate's own size; see `lawn_radius`.
            PropKind::Lawn => 80.0,
            PropKind::LilyPad => 19.0,
            PropKind::Cobble => 13.0,
            PropKind::Hide => 24.0,
        }
    }

    /// How many of one kind an enclosure will hold. A tank with nine dishes of
    /// medium in it is not a habitat, it is a warehouse — and the creature
    /// would never leave the furniture.
    pub fn max_count(&self) -> usize {
        match self {
            PropKind::Lawn | PropKind::Dish | PropKind::Bark | PropKind::Hide => 1,
            PropKind::Twig | PropKind::Food => 2,
            _ => 4,
        }
    }

    /// Does this prop belong to the *tank* rather than to the world? A lawn is
    /// poured to fill its plate, so it grows when the plate does; everything
    /// else keeps its real size, exactly as the creature does — constant
    /// apparent size is why the app is orthographic in the first place.
    pub fn scales_with_tank(&self) -> bool {
        matches!(self, PropKind::Lawn)
    }

    /// A stable name for the settings file. Separate from `label` on purpose:
    /// the label is prose and may be reworded, and a saved enclosure must not
    /// empty itself because someone improved a menu string.
    pub fn slug(&self) -> &'static str {
        match self {
            PropKind::Pebble => "pebble",
            PropKind::Plant => "plant",
            PropKind::Food => "food",
            PropKind::Dish => "dish",
            PropKind::Fruit => "fruit",
            PropKind::Vial => "vial",
            PropKind::Bark => "bark",
            PropKind::Twig => "twig",
            PropKind::Lawn => "lawn",
            PropKind::LilyPad => "lilypad",
            PropKind::Cobble => "cobble",
            PropKind::Hide => "hide",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        [
            PropKind::Pebble,
            PropKind::Plant,
            PropKind::Food,
            PropKind::Dish,
            PropKind::Fruit,
            PropKind::Vial,
            PropKind::Bark,
            PropKind::Twig,
            PropKind::Lawn,
            PropKind::LilyPad,
            PropKind::Cobble,
            PropKind::Hide,
        ]
        .into_iter()
        .find(|k| k.slug() == s)
    }

    pub fn label(&self) -> &'static str {
        match self {
            PropKind::Pebble => "Pebble",
            PropKind::Plant => "Plant",
            PropKind::Food => "Food",
            PropKind::Dish => "Dish of medium",
            PropKind::Fruit => "Fruit",
            PropKind::Vial => "Culture vial",
            PropKind::Bark => "Cork bark",
            PropKind::Twig => "Twig",
            PropKind::Lawn => "Bacterial lawn",
            PropKind::LilyPad => "Lily pad",
            PropKind::Cobble => "Cobble",
            PropKind::Hide => "Cork hide",
        }
    }

    /// Scenery never moves; anything else has physics or a life of its own.
    pub fn is_scenery(&self) -> bool {
        matches!(
            self,
            PropKind::Pebble | PropKind::Vial | PropKind::Bark | PropKind::Twig | PropKind::Cobble
        )
    }

    /// What the creature is drawn toward. Food and standing food are targets;
    /// so is a perch, for the one animal whose enclosure has no food in it —
    /// a jumping spider patrols its bark and its twig, and that is what the
    /// bark and the twig are for. The rest is not a target at all, so the
    /// creature spends most of its time being a creature rather than orbiting
    /// the furniture.
    fn rank(&self) -> i32 {
        match self {
            PropKind::Food | PropKind::Fruit | PropKind::Dish | PropKind::Lawn => 2,
            PropKind::Bark | PropKind::Twig | PropKind::Hide => 1,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Prop {
    pub kind: PropKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub radius: f32,
    /// 0..1 — how disturbed this prop is right now. Drives the plant's sway and
    /// the ball's highlight; decays on its own.
    pub stir: f32,
    /// Phase for idle motion, so two plants do not sway in lockstep.
    pub phase: f32,
    /// Food only: seconds until it reappears. 0 means it is present.
    pub respawn: f32,
    /// Which of a kind's looks this one has — banana or apple, plain pad or
    /// flowering pad. Meaningless to the physics.
    pub variant: u8,
}

impl Prop {
    pub fn present(&self) -> bool {
        self.respawn <= 0.0
    }
}

/// One prop as it goes to disk. Deliberately loose about the kind — a `String`
/// rather than the enum — so that a settings file naming something this build
/// has never heard of is a skipped prop and not a parse failure that costs the
/// user their creature's habituation history in the same file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PropSave {
    pub kind: String,
    #[serde(default)]
    pub variant: u8,
    /// Position as a fraction of the tank's half-extent, -1..1.
    #[serde(default)]
    pub nx: f32,
    #[serde(default)]
    pub ny: f32,
}

/// How close the creature has to get to a prop to affect it. Generous: the
/// bodies are a few dozen units long and the point is that contact reads as
/// contact, not as a near miss.
const TOUCH: f32 = 26.0;
/// The cursor is a shadow passing over the tank, not a finger in it — it stirs
/// props from further away, and pushes them more gently.
const CURSOR_REACH: f32 = 70.0;
/// The ceiling on contents however the user combines them. A tank this full is
/// already a picture with no room in it for the animal.
pub const MAX_PROPS: usize = 12;

/// The enclosure: a region, a kind, and the props inside it.
#[derive(Debug, Clone)]
pub struct Habitat {
    pub kind: HabitatKind,
    pub region: Region,
    pub props: Vec<Prop>,
    /// Which prop the creature is currently interested in, as an index. The
    /// bodies only ever see its position, via `World::attractor`.
    pub focus: Option<usize>,
    focus_timer: f32,
    rng: Pcg32,
}

impl Habitat {
    pub fn new(kind: HabitatKind, region: Region, seed: u64) -> Self {
        let mut h = Habitat {
            kind,
            region,
            props: Vec::new(),
            focus: None,
            focus_timer: 0.0,
            rng: Pcg32::new(seed),
        };
        h.stock();
        h
    }

    /// Fill the enclosure with its default contents. Deliberately sparse: four
    /// props in a tank the size of a browser window is already a busy picture
    /// at this scale, and the creature is the thing you are meant to be
    /// watching. The user can add more; this is where it starts.
    fn stock(&mut self) {
        self.props.clear();
        self.focus = None;
        let roster: Vec<(PropKind, u8)> = match self.kind {
            HabitatKind::FlyCage => vec![
                (PropKind::Dish, 0),
                (PropKind::Fruit, 0),
                (PropKind::Fruit, 1),
                (PropKind::Vial, 0),
            ],
            HabitatKind::Vivarium => vec![
                (PropKind::Bark, 0),
                (PropKind::Twig, 0),
                (PropKind::Plant, 0),
                (PropKind::Pebble, 0),
            ],
            HabitatKind::AgarPlate => vec![(PropKind::Lawn, 0)],
            HabitatKind::SandTerrarium => vec![
                (PropKind::Hide, 0),
                (PropKind::Cobble, 0),
                (PropKind::Plant, 0),
                (PropKind::Pebble, 0),
            ],
            HabitatKind::Pond => vec![
                (PropKind::LilyPad, 1),
                (PropKind::LilyPad, 0),
                (PropKind::LilyPad, 0),
                (PropKind::Cobble, 0),
                (PropKind::Cobble, 1),
                (PropKind::Food, 0),
            ],
        };
        for (kind, variant) in roster {
            self.push(kind, variant);
        }
    }

    /// A lawn is sized to its plate rather than to itself: a 12-unit patch of
    /// bacteria on a 500-unit dish would be a fleck.
    fn lawn_radius(&self) -> f32 {
        let (hw, hh) = self.region.half();
        (hw.min(hh) * 0.36).clamp(40.0, 120.0)
    }

    /// Put one prop of `kind` into the enclosure, in a place that suits it.
    ///
    /// Where a kind belongs is a property of the kind, not of the stocking
    /// list -- which is what lets the user add a third lily pad and have it
    /// land on the water rather than at the origin.
    fn push(&mut self, kind: PropKind, variant: u8) {
        let radius = if kind.scales_with_tank() {
            self.lawn_radius()
        } else {
            kind.default_radius()
        };
        let (hw, hh) = self.region.half();
        let c = self.region.center;
        let pos = match kind {
            // Scenery and plants belong toward the back wall (up-screen),
            // where they frame the creature instead of sitting on top of it. A
            // wide band rather than a narrow one so it does not line up on an
            // invisible shelf.
            PropKind::Plant | PropKind::Pebble | PropKind::Cobble | PropKind::Dish => Vec2::new(
                c.x + self.rng.range(-hw * 0.72, hw * 0.72),
                c.y + self.rng.range(-hh * 0.28, hh * 0.70),
            ),
            // In the back half, clear of the middle, where a snake can get
            // under it without the tank being all hide.
            PropKind::Hide => Vec2::new(
                c.x + self.rng.range(-hw * 0.6, hw * 0.6),
                c.y + self.rng.range(hh * 0.1, hh * 0.6),
            ),
            // Hard against the back wall: it leans on it.
            PropKind::Bark => Vec2::new(
                c.x + self.rng.range(-hw * 0.55, hw * 0.55),
                c.y + hh - radius * 0.7,
            ),
            // Rooted in the back half, rising toward the far top corner.
            PropKind::Twig => Vec2::new(
                c.x + self.rng.range(-hw * 0.5, hw * 0.5),
                c.y + self.rng.range(hh * 0.0, hh * 0.5),
            ),
            // Lying along a side wall, out of the middle of the floor.
            PropKind::Vial => {
                let side = if self.rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
                Vec2::new(
                    c.x + side * (hw - radius * 2.6 - 10.0).max(0.0),
                    c.y + self.rng.range(-hh * 0.5, hh * 0.5),
                )
            }
            // Spread a little off-centre, as a lawn is when it is poured from a
            // pipette rather than drawn with a compass.
            PropKind::Lawn => Vec2::new(
                c.x + self.rng.range(-hw * 0.18, hw * 0.18),
                c.y + self.rng.range(-hh * 0.18, hh * 0.18),
            ),
            _ => self.region.sample(&mut self.rng, 60.0),
        };
        let phase = self.props.len() as f32 * 1.7;
        self.props.push(Prop {
            kind,
            pos: self.region.clamp_inside(pos, radius * 0.5 + 6.0),
            vel: Vec2::ZERO,
            radius,
            stir: 0.0,
            phase,
            respawn: 0.0,
            variant,
        });
    }

    /// How many of `kind` are in here.
    pub fn count(&self, kind: PropKind) -> usize {
        self.props.iter().filter(|p| p.kind == kind).count()
    }

    /// Whether one more of `kind` would be allowed -- both per kind and against
    /// the total, so no combination of additions can bury the animal.
    pub fn can_add(&self, kind: PropKind) -> bool {
        PropKind::catalogue(self.kind).contains(&kind)
            && self.count(kind) < kind.max_count()
            && self.props.len() < MAX_PROPS
    }

    /// Add one. Variants cycle rather than repeat, so a second piece of fruit
    /// is the apple and not another banana.
    pub fn add(&mut self, kind: PropKind) -> bool {
        if !self.can_add(kind) {
            return false;
        }
        let variant = (self.count(kind) % 2) as u8;
        self.push(kind, variant);
        true
    }

    /// Remove the most recently added one of `kind`.
    ///
    /// The focus is dropped rather than repaired: it is an index into this
    /// vector, and a stale one would point the creature at whatever slid into
    /// the gap. `retarget` picks a new one on the next frame anyway.
    pub fn remove(&mut self, kind: PropKind) -> bool {
        match self.props.iter().rposition(|p| p.kind == kind) {
            Some(i) => {
                self.props.remove(i);
                self.focus = None;
                self.focus_timer = 0.0;
                true
            }
            None => false,
        }
    }

    /// The contents, in a form that survives a restart: what each prop is, and
    /// where it sits as a *fraction* of the tank rather than in scene units.
    ///
    /// Normalised on purpose. The tank is placed against the display, so its
    /// centre and size differ between a laptop panel and a 1440p monitor and
    /// between one zoom setting and another; absolute positions would restore
    /// a carefully-arranged pond onto the desk beside it.
    pub fn snapshot(&self) -> Vec<PropSave> {
        let (hw, hh) = self.region.half();
        self.props
            .iter()
            .map(|p| PropSave {
                kind: p.kind.slug().to_string(),
                variant: p.variant,
                nx: if hw > 1e-3 { (p.pos.x - self.region.center.x) / hw } else { 0.0 },
                ny: if hh > 1e-3 { (p.pos.y - self.region.center.y) / hh } else { 0.0 },
            })
            .collect()
    }

    /// Put back what `snapshot` saved. Anything unrecognised, out of catalogue
    /// or over a limit is dropped rather than rejected wholesale: a settings
    /// file edited by hand, or written by an older build, should cost you one
    /// prop and not your whole enclosure.
    ///
    /// An empty result is honoured — an emptied tank is a legitimate choice, and
    /// re-stocking it on every launch would make that choice impossible to keep.
    pub fn restore(&mut self, saved: &[PropSave]) {
        self.props.clear();
        self.focus = None;
        self.focus_timer = 0.0;
        let (hw, hh) = self.region.half();
        for sp in saved {
            let Some(kind) = PropKind::from_slug(&sp.kind) else { continue };
            if !self.can_add(kind) {
                continue;
            }
            self.push(kind, sp.variant);
            if let Some(p) = self.props.last_mut() {
                p.pos = Vec2::new(
                    self.region.center.x + sp.nx.clamp(-1.0, 1.0) * hw,
                    self.region.center.y + sp.ny.clamp(-1.0, 1.0) * hh,
                );
                let m = p.radius * 0.5 + 6.0;
                p.pos = self.region.clamp_inside(p.pos, m);
            }
        }
    }

    /// Put the enclosure back to the contents it ships with.
    pub fn restock(&mut self) {
        self.stock();
    }

    /// Move or resize the enclosure -- on a display change, when the user zooms
    /// it, or when the screen geometry shifts under it.
    ///
    /// Props travel with their tank, and a *resize* now carries them
    /// proportionally rather than restocking. Restocking was fine when the only
    /// resize was a display change; with a zoom control it would reshuffle the
    /// furniture on every frame the user held the chord, which is both ugly and
    /// a way to lose a lily pad you had watched the fish push into a corner.
    /// Radii stay put as the tank grows: the creature's apparent size is
    /// constant by design, and a prop that grew with the tank would outrun it.
    pub fn reshape(&mut self, region: Region) {
        let old = self.region;
        self.region = region;
        let kx = if old.size.0 > 1e-3 { region.size.0 / old.size.0 } else { 1.0 };
        let ky = if old.size.1 > 1e-3 { region.size.1 / old.size.1 } else { 1.0 };
        let lawn = self.lawn_radius();
        for p in self.props.iter_mut() {
            p.pos = Vec2::new(
                region.center.x + (p.pos.x - old.center.x) * kx,
                region.center.y + (p.pos.y - old.center.y) * ky,
            );
            if p.kind.scales_with_tank() {
                p.radius = lawn;
            }
            p.pos = region.clamp_inside(p.pos, p.radius * 0.5 + 6.0);
        }
    }

    /// One frame of enclosure life.
    pub fn step(&mut self, dt: f32, creature: Vec2, cursor: Option<Vec2>) {
        let region = self.region;
        for p in self.props.iter_mut() {
            p.phase += dt * (0.7 + p.radius * 0.02);
            p.stir = (p.stir - dt * 0.8).max(0.0);

            if p.respawn > 0.0 {
                p.respawn -= dt;
                continue;
            }

            let d = hypot(creature.x - p.pos.x, creature.y - p.pos.y);
            let near_creature = d < p.radius + TOUCH;
            let near_cursor = cursor
                .map(|c| hypot(c.x - p.pos.x, c.y - p.pos.y) < p.radius + CURSOR_REACH)
                .unwrap_or(false);
            if near_creature || near_cursor {
                p.stir = (p.stir + dt * 3.0).min(1.0);
            }

            match p.kind {
                PropKind::LilyPad => {
                    // Pushed away from whatever touched it, then friction. The
                    // pad is the one prop that keeps a position of its own, so
                    // a session leaves it wherever the fish nosed it. Water
                    // damps it hard and the stones give almost nothing back,
                    // and it has a slow drift of its own so a still pond is
                    // never quite still.
                    let (shove, nudge, drag, bounce) = (60.0, 30.0, 1.6, 0.25);
                    if near_creature && d > 1e-3 {
                        let k = shove * (1.0 - d / (p.radius + TOUCH)).max(0.0);
                        p.vel.x += (p.pos.x - creature.x) / d * k * dt;
                        p.vel.y += (p.pos.y - creature.y) / d * k * dt;
                    }
                    if let Some(c) = cursor {
                        let cd = hypot(c.x - p.pos.x, c.y - p.pos.y);
                        if cd < p.radius + CURSOR_REACH && cd > 1e-3 {
                            let k = nudge * (1.0 - cd / (p.radius + CURSOR_REACH)).max(0.0);
                            p.vel.x += (p.pos.x - c.x) / cd * k * dt;
                            p.vel.y += (p.pos.y - c.y) / cd * k * dt;
                        }
                    }
                    p.vel.x += (p.phase * 0.31).sin() * 0.9 * dt;
                    p.vel.y += (p.phase * 0.23 + 1.1).cos() * 0.9 * dt;
                    let damp = (1.0 - drag * dt).max(0.0);
                    p.vel.x *= damp;
                    p.vel.y *= damp;
                    p.pos.x += p.vel.x * dt;
                    p.pos.y += p.vel.y * dt;
                    // Bounce off the edge rather than stopping dead against it.
                    let (hw, hh) = region.half();
                    let ex = (hw - p.radius - 6.0).max(0.0);
                    let ey = (hh - p.radius - 6.0).max(0.0);
                    if (p.pos.x - region.center.x).abs() > ex {
                        p.vel.x = -p.vel.x * bounce;
                    }
                    if (p.pos.y - region.center.y).abs() > ey {
                        p.vel.y = -p.vel.y * bounce;
                    }
                    p.pos = region.clamp_inside(p.pos, p.radius + 6.0);
                }
                PropKind::Food => {
                    // A pellet on the surface: drifts toward the near edge,
                    // wanders a little on the way, and is gone the moment the
                    // fish reaches it.
                    p.pos.y -= 11.0 * dt;
                    p.pos.x += (p.phase * 0.9).sin() * 6.0 * dt;
                    if near_creature {
                        p.respawn = 6.0;
                    } else if p.pos.y <= region.center.y - region.half().1 + 26.0 {
                        // Reached the edge uneaten. Rather than leaving a
                        // pellet parked against the stones forever, it goes
                        // soggy and a fresh one is dropped in later.
                        p.respawn = 5.0;
                    }
                    p.pos = region.clamp_inside(p.pos, 24.0);
                }
                // Anchored, or scenery: the plant sways and the fruit is fed
                // on, but neither goes anywhere.
                _ => {}
            }
        }

        // A flake that has just come back starts near the surface, where food
        // would fall in from.
        for i in 0..self.props.len() {
            if self.props[i].kind == PropKind::Food && self.props[i].respawn > 0.0 {
                let (hw, hh) = self.region.half();
                let x = self.region.center.x + self.rng.range(-hw * 0.6, hw * 0.6);
                let p = &mut self.props[i];
                if p.pos.y < self.region.center.y + hh - 41.0 {
                    p.pos = Vec2::new(x, self.region.center.y + hh - 40.0);
                }
            }
        }

        self.retarget(dt, creature);
    }

    /// Pick something for the creature to head toward, and stick with it for a
    /// while. Re-picking every frame would make the creature jitter between
    /// props; holding one for several seconds reads as having decided.
    fn retarget(&mut self, dt: f32, creature: Vec2) {
        self.focus_timer -= dt;
        let stale = match self.focus {
            None => true,
            Some(i) => {
                self.focus_timer <= 0.0
                    || !self.props[i].present()
                    || hypot(creature.x - self.props[i].pos.x, creature.y - self.props[i].pos.y)
                        < self.props[i].radius + TOUCH
            }
        };
        if !stale {
            return;
        }
        // Food is a target and scenery is not, so the creature spends most of
        // its time being a creature rather than orbiting the furniture. Among
        // equals — the fly's dish and its two pieces of fruit — the pick is
        // random, so it visits all of them over a session instead of living on
        // the first one in the list.
        let top = self
            .props
            .iter()
            .filter(|p| p.present())
            .map(|p| p.kind.rank())
            .max()
            .unwrap_or(0);
        let candidates: Vec<usize> = self
            .props
            .iter()
            .enumerate()
            .filter(|(_, p)| p.present() && top > 0 && p.kind.rank() == top)
            .map(|(i, _)| i)
            .collect();
        self.focus = match candidates.len() {
            0 => None,
            1 => Some(candidates[0]),
            n => {
                let k = (self.rng.range(0.0, n as f32) as usize).min(n - 1);
                Some(candidates[k])
            }
        };
        self.focus_timer = self.rng.range(4.0, 9.0);
    }

    /// Where the creature is currently drawn to, if anywhere. This is the only
    /// thing the bodies ever learn about the props.
    pub fn attractor(&self) -> Option<Vec2> {
        self.focus
            .filter(|&i| self.props[i].present())
            .map(|i| self.props[i].pos)
    }
}

// Where a tank goes on the screen is a *camera* question, not a geometry one:
// under a tilted, yawed view the ground rectangle and its screen footprint are
// different shapes. That logic lives in the shell's `camera` module, next to the
// projection it depends on.

#[cfg(test)]
mod tests {
    use super::*;

    fn tank(kind: HabitatKind) -> Habitat {
        Habitat::new(kind, Region::new(Vec2::new(300.0, -200.0), (600.0, 420.0)), 7)
    }

    /// The zoom control resizes the tank on every frame the chord is held. If a
    /// resize restocked — as it used to, when the only resize was a display
    /// change — the furniture would reshuffle continuously while the user
    /// dragged, and anything the creature had pushed somewhere would be lost.
    #[test]
    fn resizing_carries_the_props_instead_of_restocking_them() {
        let mut h = tank(HabitatKind::Pond);
        let before: Vec<(PropKind, u8)> = h.props.iter().map(|p| (p.kind, p.variant)).collect();
        // Where one prop sits as a fraction of the tank, which is what must be
        // preserved through a resize.
        let i = h.props.iter().position(|p| p.kind == PropKind::Cobble).unwrap();
        let (hw, hh) = h.region.half();
        let frac = (
            (h.props[i].pos.x - h.region.center.x) / hw,
            (h.props[i].pos.y - h.region.center.y) / hh,
        );

        h.reshape(Region::new(Vec2::new(-100.0, 40.0), (900.0, 630.0)));

        let after: Vec<(PropKind, u8)> = h.props.iter().map(|p| (p.kind, p.variant)).collect();
        assert_eq!(before, after, "a resize changed the contents");
        let (hw2, hh2) = h.region.half();
        let frac2 = (
            (h.props[i].pos.x - h.region.center.x) / hw2,
            (h.props[i].pos.y - h.region.center.y) / hh2,
        );
        assert!(
            (frac.0 - frac2.0).abs() < 1e-3 && (frac.1 - frac2.1).abs() < 1e-3,
            "the prop moved within its tank: {frac:?} became {frac2:?}"
        );
        // Nothing may end up outside the new walls.
        for p in &h.props {
            assert!(!h.region.outside(p.pos, 0.0), "a prop is outside the resized tank");
        }
    }

    /// The one prop that is part of its container rather than part of the world:
    /// a lawn is poured to fill its plate. Everything else keeps its real size,
    /// as the creature does.
    #[test]
    fn only_the_lawn_grows_with_its_tank() {
        let mut h = tank(HabitatKind::AgarPlate);
        let lawn0 = h.props[0].radius;
        h.reshape(Region::new(h.region.center, (1200.0, 840.0)));
        assert!(h.props[0].radius > lawn0, "the lawn did not grow with the plate");

        let mut cage = tank(HabitatKind::FlyCage);
        let sizes: Vec<f32> = cage.props.iter().map(|p| p.radius).collect();
        cage.reshape(Region::new(cage.region.center, (1200.0, 840.0)));
        let after: Vec<f32> = cage.props.iter().map(|p| p.radius).collect();
        assert_eq!(sizes, after, "a cage prop changed size with the tank");
    }

    /// Adding and removing has to stay inside what the enclosure can hold, and
    /// what belongs in it — cork bark in a pond is a mistake, not a choice.
    #[test]
    fn contents_can_be_managed_but_not_abused() {
        let mut h = tank(HabitatKind::Pond);
        assert!(!h.can_add(PropKind::Bark), "a pond accepted cork bark");
        assert!(!h.add(PropKind::Bark));

        // Fill every catalogue slot as hard as it will go.
        for _ in 0..50 {
            for k in PropKind::catalogue(HabitatKind::Pond) {
                h.add(*k);
            }
        }
        assert!(h.props.len() <= MAX_PROPS, "{} props in one pond", h.props.len());
        for k in PropKind::catalogue(HabitatKind::Pond) {
            assert!(h.count(*k) <= k.max_count(), "too many {}", k.label());
        }

        // And it can be emptied, one at a time, without panicking on the last.
        for _ in 0..MAX_PROPS + 4 {
            for k in PropKind::catalogue(HabitatKind::Pond) {
                h.remove(*k);
            }
        }
        assert!(h.props.is_empty(), "the pond would not empty");
        assert_eq!(h.attractor(), None, "an empty tank still had something to aim at");
        // An emptied tank must survive being stepped: the focus index is the
        // thing that would dangle.
        h.step(0.016, Vec2::ZERO, Some(Vec2::new(10.0, 10.0)));
        assert_eq!(h.attractor(), None);

        h.restock();
        assert!(!h.props.is_empty(), "restock left the pond empty");
    }

    /// Removing a prop must not leave the creature aimed at whatever slid into
    /// its slot — the focus is an index, and a stale one points at a stranger.
    #[test]
    fn removing_a_prop_drops_a_stale_focus() {
        let mut h = tank(HabitatKind::FlyCage);
        // Run until it has decided on something.
        for _ in 0..20 {
            h.step(0.1, Vec2::new(0.0, 0.0), None);
        }
        assert!(h.focus.is_some(), "nothing was ever focused on");
        h.remove(PropKind::Fruit);
        assert_eq!(h.focus, None, "the focus outlived the prop list it indexes");
        // And it recovers on its own.
        for _ in 0..20 {
            h.step(0.1, Vec2::new(0.0, 0.0), None);
        }
        for i in h.focus {
            assert!(i < h.props.len(), "focus {i} is out of range");
        }
    }

    /// An arrangement has to come back as it was left — on a differently-sized
    /// tank, which is the case absolute coordinates would get wrong.
    #[test]
    fn an_arranged_tank_survives_a_round_trip_through_disk() {
        let mut h = tank(HabitatKind::Vivarium);
        h.add(PropKind::Pebble);
        h.remove(PropKind::Plant);
        let saved = h.snapshot();
        let want: Vec<(PropKind, u8)> = h.props.iter().map(|p| (p.kind, p.variant)).collect();
        let fracs: Vec<(f32, f32)> = {
            let (hw, hh) = h.region.half();
            h.props
                .iter()
                .map(|p| ((p.pos.x - h.region.center.x) / hw, (p.pos.y - h.region.center.y) / hh))
                .collect()
        };

        // A different display: bigger tank, somewhere else entirely.
        let mut other = Habitat::new(
            HabitatKind::Vivarium,
            Region::new(Vec2::new(-700.0, 380.0), (880.0, 610.0)),
            99,
        );
        other.restore(&saved);

        let got: Vec<(PropKind, u8)> = other.props.iter().map(|p| (p.kind, p.variant)).collect();
        assert_eq!(want, got, "the arrangement came back as something else");
        let (hw, hh) = other.region.half();
        for (i, p) in other.props.iter().enumerate() {
            let f = ((p.pos.x - other.region.center.x) / hw, (p.pos.y - other.region.center.y) / hh);
            assert!(
                (f.0 - fracs[i].0).abs() < 0.02 && (f.1 - fracs[i].1).abs() < 0.02,
                "prop {i} restored to {f:?} rather than {:?}",
                fracs[i]
            );
        }
    }

    /// A settings file that is nonsense, hand-edited or from another build must
    /// cost at most the props it got wrong.
    #[test]
    fn a_corrupt_arrangement_costs_only_the_props_it_names_wrongly() {
        let mut h = tank(HabitatKind::Pond);
        let saved = vec![
            PropSave { kind: "lilypad".into(), variant: 0, nx: 0.2, ny: -0.3 },
            PropSave { kind: "unicorn".into(), variant: 0, nx: 0.0, ny: 0.0 },
            // Belongs to another enclosure entirely.
            PropSave { kind: "bark".into(), variant: 0, nx: 0.0, ny: 0.0 },
            // Off the end of the world.
            PropSave { kind: "cobble".into(), variant: 0, nx: 44.0, ny: -91.0 },
        ];
        h.restore(&saved);
        assert_eq!(h.props.len(), 2, "expected the lily pad and the cobble only");
        for p in &h.props {
            assert!(!h.region.outside(p.pos, 0.0), "a restored prop is outside the pond");
        }
    }

    /// An empty saved tank is a choice, not a failure: restoring it must not
    /// quietly restock, or emptying an enclosure would be impossible to keep.
    #[test]
    fn an_emptied_tank_stays_empty_across_a_restore() {
        let mut h = tank(HabitatKind::FlyCage);
        h.restore(&[]);
        assert!(h.props.is_empty());
    }

    /// The whole feature rests on this: a centred region must give the bodies
    /// the same numbers the old `±bounds/2` arithmetic did, or turning habitat
    /// mode *off* would no longer be free-roam.
    #[test]
    fn a_centred_region_reproduces_the_old_bounds_arithmetic() {
        let size = (1512.0, 982.0);
        let r = Region::centered(size);
        for &(x, y) in &[
            (700.0, 400.0),
            (-755.0, 491.0),
            (900.0, -600.0),
            (-100.0, 480.0),
        ] {
            let p = Vec2::new(x, y);
            let margin = 24.0;
            let hw = size.0 / 2.0 - margin;
            let hh = size.1 / 2.0 - margin;
            assert_eq!(
                r.outside(p, margin),
                p.x.abs() > hw || p.y.abs() > hh,
                "edge test disagrees at ({x}, {y})"
            );
            let c = r.clamp_inside(p, 20.0);
            assert_eq!(c.x, clamp(p.x, -size.0 / 2.0 + 20.0, size.0 / 2.0 - 20.0));
            assert_eq!(c.y, clamp(p.y, -size.1 / 2.0 + 20.0, size.1 / 2.0 - 20.0));
            assert_eq!(r.bearing_home(p), (-p.y).atan2(-p.x));
        }

        // The exact centre is the one point where the two formulations
        // disagree, and only over signed zero: `(-0.0).atan2(-0.0)` is -pi,
        // while `(0.0 - 0.0).atan2(0.0 - 0.0)` is 0.0. "Which way is home" is
        // undefined when you are already home, and no body can reach this case
        // anyway — it would have to be at the centre *and* within `margin` of a
        // wall, which needs a region narrower than two margins.
        assert_eq!(r.bearing_home(Vec2::ZERO), 0.0);
    }

    #[test]
    fn an_offset_region_confines_to_itself_not_to_the_origin() {
        let r = Region::new(Vec2::new(400.0, -250.0), (500.0, 300.0));
        let c = r.clamp_inside(Vec2::ZERO, 20.0);
        // Centre (400, -250), half (250, 150), margin 20 -> the inside runs
        // x 170..630 and y -380..-120. The origin is off to the upper left of
        // that, so it lands on the corner.
        assert!(
            (c.x - 170.0).abs() < 1e-3 && (c.y + 120.0).abs() < 1e-3,
            "clamped to ({}, {})",
            c.x,
            c.y
        );
        assert!(!r.outside(r.center, 20.0));
        assert!(r.outside(Vec2::ZERO, 20.0));
    }

    /// Substrate finally does something: each way of moving gets the container
    /// that animal is actually kept in.
    #[test]
    fn each_substrate_gets_its_own_enclosure() {
        assert_eq!(
            HabitatKind::for_substrate(Substrate::WalkerFlier),
            HabitatKind::FlyCage
        );
        assert_eq!(
            HabitatKind::for_substrate(Substrate::WalkerJumper),
            HabitatKind::Vivarium
        );
        assert_eq!(
            HabitatKind::for_substrate(Substrate::Crawler),
            HabitatKind::AgarPlate
        );
        assert_eq!(HabitatKind::for_substrate(Substrate::Swimmer), HabitatKind::Pond);
        assert_eq!(
            HabitatKind::for_substrate(Substrate::Burrower),
            HabitatKind::SandTerrarium
        );
        // And only the pond holds water.
        for k in HabitatKind::ALL {
            assert_eq!(k.is_wet(), k == HabitatKind::Pond, "{k:?}");
        }
    }

    #[test]
    fn props_are_stocked_inside_the_tank() {
        for kind in HabitatKind::ALL {
            let r = Region::new(Vec2::new(300.0, -200.0), (600.0, 400.0));
            let h = Habitat::new(kind, r, 9);
            assert!(!h.props.is_empty(), "{kind:?} is empty");
            for p in &h.props {
                assert!(
                    !r.outside(p.pos, 0.0),
                    "{:?} prop {:?} spawned outside the tank at ({}, {})",
                    kind,
                    p.kind,
                    p.pos.x,
                    p.pos.y
                );
            }
        }
        // The stock differs by kind, or "matched to the creature type" means
        // nothing: each enclosure has something the others do not.
        let r = Region::centered((600.0, 400.0));
        let has = |k: HabitatKind, pk: PropKind| {
            Habitat::new(k, r, 1).props.iter().any(|p| p.kind == pk)
        };
        assert!(has(HabitatKind::FlyCage, PropKind::Dish));
        assert!(has(HabitatKind::FlyCage, PropKind::Fruit));
        assert!(has(HabitatKind::Vivarium, PropKind::Bark));
        assert!(has(HabitatKind::Vivarium, PropKind::Twig));
        assert!(has(HabitatKind::AgarPlate, PropKind::Lawn));
        assert!(has(HabitatKind::Pond, PropKind::LilyPad));
        assert!(has(HabitatKind::Pond, PropKind::Food));
        for k in HabitatKind::ALL {
            // Food that floats belongs on water and nowhere else.
            assert_eq!(has(k, PropKind::Food), k.is_wet(), "{k:?}");
            assert_eq!(has(k, PropKind::LilyPad), k.is_wet(), "{k:?}");
        }
        // The worm's plate is a lawn and nothing else — there is nothing else
        // on a real one.
        assert_eq!(Habitat::new(HabitatKind::AgarPlate, r, 1).props.len(), 1);
    }

    /// Every enclosure gives the creature something to head for — even the
    /// bare plate, whose lawn is exactly what a worm crawls toward.
    #[test]
    fn every_enclosure_has_a_standing_target() {
        let r = Region::centered((600.0, 400.0));
        for kind in HabitatKind::ALL {
            let mut h = Habitat::new(kind, r, 4);
            h.step(1.0 / 60.0, Vec2::new(-2000.0, -2000.0), None);
            let a = h
                .attractor()
                .unwrap_or_else(|| panic!("{kind:?} offers nothing"));
            assert!(!r.outside(a, 0.0), "{kind:?} target is outside the tank");
        }
        // Standing food is not used up: the fly can feed on the fruit all day.
        let mut h = Habitat::new(HabitatKind::FlyCage, r, 4);
        h.step(1.0 / 60.0, Vec2::ZERO, None);
        for _ in 0..600 {
            let at = h.attractor().expect("the cage has food");
            h.step(1.0 / 60.0, at, None);
        }
        assert!(
            h.props.iter().all(|p| p.present()),
            "the cage ran out of food"
        );
    }

    /// The lily pad is the "play with it" prop: swimming into it must move it,
    /// and it must then come to rest rather than drifting away forever.
    #[test]
    fn the_creature_pushes_the_pad_and_the_pad_settles() {
        let r = Region::centered((600.0, 400.0));
        let mut h = Habitat::new(HabitatKind::Pond, r, 3);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::LilyPad)
            .unwrap();
        h.props[i].pos = Vec2::ZERO;
        h.props[i].vel = Vec2::ZERO;

        // Creature arrives from the left and leans on it.
        for _ in 0..30 {
            h.step(1.0 / 60.0, Vec2::new(-20.0, 0.0), None);
        }
        let pushed = h.props[i].pos;
        assert!(
            pushed.x > 3.0,
            "the pad did not move away from the creature: ({}, {})",
            pushed.x,
            pushed.y
        );

        // Left alone it coasts to a stop, and stays inside the stones. "Stop"
        // allows for the idle drift that keeps the pond from looking frozen.
        for _ in 0..600 {
            h.step(1.0 / 60.0, Vec2::new(-2000.0, 0.0), None);
        }
        let v = hypot(h.props[i].vel.x, h.props[i].vel.y);
        assert!(v < 2.0, "the pad never settled: {v}");
        assert!(!r.outside(h.props[i].pos, 0.0), "the pad escaped the pond");
        // Scenery, by contrast, has not moved at all.
        for p in h.props.iter().filter(|p| p.kind.is_scenery()) {
            assert!(p.vel.x == 0.0 && p.vel.y == 0.0, "{:?} drifted", p.kind);
        }
    }

    /// The other half of "notices, approaches": the fish is pointed at the food,
    /// and once it reaches it the pellet is gone and stops being a target.
    #[test]
    fn food_is_a_target_until_it_is_eaten() {
        let r = Region::centered((600.0, 400.0));
        let mut h = Habitat::new(HabitatKind::Pond, r, 5);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Food)
            .unwrap();
        h.props[i].pos = Vec2::new(100.0, 40.0);
        h.step(1.0 / 60.0, Vec2::new(-200.0, -100.0), None);
        let a = h.attractor().expect("the fish should be drawn to the pellet");
        assert!(
            (a.x - h.props[i].pos.x).abs() < 1e-3,
            "attracted to the wrong prop"
        );

        // Swim into it.
        let at = h.props[i].pos;
        for _ in 0..5 {
            h.step(1.0 / 60.0, at, None);
        }
        assert!(!h.props[i].present(), "the pellet was not eaten");
        assert!(
            h.attractor().is_none(),
            "an eaten pellet is still being chased"
        );
    }

    /// A cursor passing over the glass stirs the plants — the only way the user
    /// touches the tank, since the overlay never takes a click.
    #[test]
    fn the_cursor_stirs_props_without_being_clicked() {
        let r = Region::centered((600.0, 400.0));
        let mut h = Habitat::new(HabitatKind::Vivarium, r, 2);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Plant)
            .unwrap();
        let at = h.props[i].pos;
        for _ in 0..20 {
            h.step(1.0 / 60.0, Vec2::new(-2000.0, -2000.0), Some(at));
        }
        assert!(h.props[i].stir > 0.2, "the plant ignored the cursor");
        // And it is still anchored — a plant does not roll away.
        assert_eq!(h.props[i].pos.x, at.x);
        assert_eq!(h.props[i].pos.y, at.y);
    }

}
