//! Menu rows AppKit adds to the menu bar by itself, switched off through the app's user defaults
//! (macOS).
//!
//! AppKit puts "Send <app> Feedback" at the top of the Help menu for every app (Feedback
//! Assistant's entry), beside LightCraft's own Help ▸ Send Feedback…. Setting `NSShowFeedbackMenu`
//! to `false` removes it (read on macOS 27 from the app's menu bar).
//!
//! AppKit also inserts its own "Enter Full Screen" row into the View menu (measured: not when
//! LightCraft runs in German, so the menu bar had two rows in English and one in German).
//! LightCraft's row (the menu model's `view.enterFullScreen`) is the one row in every language: it
//! reads "Exit Full Screen" while the window is full screen and takes the system key ⌃⌘F. So
//! `NSFullScreenMenuItemEverywhere` is set to `false` and AppKit adds none.
//!
//! AppKit reads these defaults when it builds the menus, so they are set before the menu bar
//! exists. The values are written to the app's preferences (the safe `setBool:forKey:`; registering
//! them only for the process takes `unsafe`, which this crate forbids) and written again at every
//! start, except in a `--memory` session, which writes nothing (issue #164); a run with
//! `LIGHTCRAFT_NO_PREFS` still writes them.

use objc2_foundation::{NSString, NSUserDefaults};

/// The user defaults that stop AppKit adding a row, each set to `false`.
const OFF: &[&str] = &["NSShowFeedbackMenu", "NSFullScreenMenuItemEverywhere"];

/// Writes [`OFF`] into `defaults`.
fn switch_off(defaults: &NSUserDefaults) {
    for key in OFF {
        defaults.setBool_forKey(false, &NSString::from_str(key));
    }
}

/// Switches off the rows in [`OFF`] for this app. Call before the menu bar is installed.
pub fn install() {
    switch_off(&NSUserDefaults::standardUserDefaults());
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::AllocAnyThread;

    /// Removes the throwaway domain, also when an assertion fails.
    struct Domain(objc2::rc::Retained<NSString>, objc2::rc::Retained<NSUserDefaults>);
    impl Drop for Domain {
        fn drop(&mut self) {
            self.1.removePersistentDomainForName(&self.0);
        }
    }

    /// A throwaway domain: the tests never touch the real preferences of the app.
    fn throwaway(name: &str) -> (Domain, objc2::rc::Retained<NSUserDefaults>) {
        let suite = NSString::from_str(&format!("com.storytold.lightcraft.test.appkit-defaults.{name}.{}", std::process::id()));
        let defaults = NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&suite)).expect("a test domain");
        (Domain(suite, defaults.clone()), defaults)
    }

    #[test]
    fn the_system_feedback_row_is_switched_off() {
        let (_domain, defaults) = throwaway("feedback");
        let key = NSString::from_str("NSShowFeedbackMenu");
        // the row shows while the default is on (AppKit registers `true`, measured); set it so the
        // test doesn't depend on the developer's own defaults
        defaults.setBool_forKey(true, &key);
        assert!(defaults.boolForKey(&key), "the row shows until the default says otherwise");
        switch_off(&defaults);
        assert!(!defaults.boolForKey(&key), "AppKit's Send Feedback row is off");
    }

    #[test]
    fn appkits_own_full_screen_row_is_switched_off() {
        let (_domain, defaults) = throwaway("fullscreen");
        let key = NSString::from_str("NSFullScreenMenuItemEverywhere");
        assert!(defaults.objectForKey(&key).is_none(), "nothing sets it until the call");
        switch_off(&defaults);
        assert!(defaults.objectForKey(&key).is_some(), "the key is set, not just absent");
        assert!(!defaults.boolForKey(&key), "the default AppKit reads is off");
    }
}
