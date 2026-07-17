//! macOS-only window chrome fixes for undecorated (`decorations: false`) windows.
//!
//! Borderless NSWindows do not get the system rounded corners that decorated
//! apps inherit. Clip the content view's layer to the standard macOS radius so
//! the shell matches native apps (and keeps `shadow: true` looking correct).

#![cfg(target_os = "macos")]

use tauri::WebviewWindow;

/// System-like corner radius for Big Sur+ windows (points).
const WINDOW_CORNER_RADIUS: f64 = 10.0;

/// Apply native rounded corners + shadow to an undecorated window.
///
/// Best-effort: failures are logged and ignored so a missing AppKit symbol
/// never blocks app launch.
pub fn apply_rounded_corners(window: &WebviewWindow) {
    let Ok(ns_window) = window.ns_window() else {
        tracing::warn!("macos_window: ns_window() unavailable; skipping rounded corners");
        return;
    };
    if ns_window.is_null() {
        tracing::warn!("macos_window: null NSWindow; skipping rounded corners");
        return;
    }

    unsafe {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;

        let ns_window = &*ns_window.cast::<AnyObject>();

        // Keep the native drop shadow that `shadow: true` requested — borderless
        // windows sometimes lose it when the content layer starts masking.
        let _: () = msg_send![ns_window, setHasShadow: true];

        let content_view: *mut AnyObject = msg_send![ns_window, contentView];
        if content_view.is_null() {
            tracing::warn!("macos_window: null contentView; skipping rounded corners");
            return;
        }
        let content_view = &*content_view;

        let _: () = msg_send![content_view, setWantsLayer: true];
        let layer: *mut AnyObject = msg_send![content_view, layer];
        if layer.is_null() {
            tracing::warn!("macos_window: null CALayer; skipping rounded corners");
            return;
        }
        let layer = &*layer;
        let _: () = msg_send![layer, setCornerRadius: WINDOW_CORNER_RADIUS];
        let _: () = msg_send![layer, setMasksToBounds: true];
    }

    tracing::debug!(
        radius = WINDOW_CORNER_RADIUS,
        "macos_window: applied rounded corners"
    );
}
