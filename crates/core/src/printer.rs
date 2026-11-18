//! The printer profile the product prints for: the printability limits (walls, holes, features)
//! come from it through the printability Law (the dependency's), and the G-code export uses it

pub use alice_bamboo::SlicerConfig;
pub use alice_bamboo::dfam::ProcessLimits;

/// The selected printer profile. The app has no printer selection yet, so this is the Bambu Lab
/// preset by name (`SlicerConfig::bambu()`: 0.4 mm nozzle, 3 perimeters), the same profile the
/// G-code export slices with
#[must_use]
pub fn selected() -> SlicerConfig {
    SlicerConfig::bambu()
}
