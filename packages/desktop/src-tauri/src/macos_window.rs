
#![cfg(target_os = "macos")]

use tauri::WebviewWindow;
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};

pub const WINDOW_CORNER_RADIUS: f64 = 10.0;

pub fn apply_macos_chrome(window: &WebviewWindow) {
    apply_window_vibrancy(window);
    apply_rounded_corners(window);
}

fn apply_window_vibrancy(window: &WebviewWindow) {
    match apply_vibrancy(
        window,
        NSVisualEffectMaterial::HudWindow,
        Some(NSVisualEffectState::Active),
        Some(WINDOW_CORNER_RADIUS),
    ) {
        Ok(()) => tracing::debug!(
            radius = WINDOW_CORNER_RADIUS,
            "macos_window: applied vibrancy"
        ),
        Err(err) => tracing::warn!(
            error = %err,
            "macos_window: apply_vibrancy failed; continuing with layer clip only"
        ),
    }
}

fn apply_rounded_corners(window: &WebviewWindow) {
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
