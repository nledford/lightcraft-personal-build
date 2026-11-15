//! The integration tests, one module per file, linked into one test binary.
//! `tests/shutdown.rs` stays a binary of its own: it closes the GPU for the whole process. So does
//! `tests/a11y_audit`, the two whole-app accessibility audits: they run alone, not beside these short tests.

#[path = "../a11y_support/mod.rs"]
mod a11y_support;
mod accessibility;
mod command_roles;
mod crop_keyboard;
mod dialog_role;
mod docking_workflows;
mod filmstrip_names;
mod grid_names;
mod landmarks;
mod local_places;
mod photo_name;
mod sidebar_folders_import;
mod sidebar_names;
mod thumb_cells;
mod thumb_size_slider;
