//! Native save-dialog titles — mirrors `dialog.*` keys in `src/i18n/messages.ts`.
//! Native dialogs cannot use the frontend catalog; keep both sides in sync.

use crate::store;

// Re-use the Locale enum from tray_i18n to keep parsing rules in one place.
pub use crate::tray_i18n::Locale;

/// Current app locale from durable settings.
pub fn app_locale() -> Locale {
    crate::tray_i18n::app_locale()
}

/// Static dialog strings for one locale.
pub struct DialogStrings {
    /// Title for the support bundle save dialog (commands::export_support_bundle).
    pub support_bundle_title: &'static str,
    /// Title for the session diagnostic bundle save dialog (commands::export_session_bundle).
    pub session_bundle_title: &'static str,
}

const EN: DialogStrings = DialogStrings {
    support_bundle_title: "Save support bundle",
    session_bundle_title: "Save session diagnostic bundle",
};

const ZH: DialogStrings = DialogStrings {
    support_bundle_title: "保存支持诊断包",
    session_bundle_title: "保存会话诊断包",
};

const ZH_TW: DialogStrings = DialogStrings {
    support_bundle_title: "儲存支援診斷包",
    session_bundle_title: "儲存工作階段診斷包",
};

pub fn strings(locale: Locale) -> &'static DialogStrings {
    match locale {
        Locale::En => &EN,
        Locale::Zh => &ZH,
        Locale::ZhTw => &ZH_TW,
    }
}

pub fn t() -> &'static DialogStrings {
    strings(app_locale())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_locales_have_nonempty_titles() {
        for loc in [Locale::En, Locale::Zh, Locale::ZhTw] {
            let s = strings(loc);
            assert!(!s.support_bundle_title.is_empty(), "{:?}.support_bundle_title", loc);
            assert!(!s.session_bundle_title.is_empty(), "{:?}.session_bundle_title", loc);
        }
    }

    #[test]
    fn zh_and_zh_tw_distinct_from_en() {
        assert_ne!(EN.support_bundle_title, ZH.support_bundle_title);
        assert_ne!(EN.support_bundle_title, ZH_TW.support_bundle_title);
        assert_ne!(ZH.support_bundle_title, ZH_TW.support_bundle_title);
    }

    #[test]
    fn app_locale_reads_settings_default() {
        // Default AppSettings.locale is "en"; app_locale must return Locale::En
        // without panicking. This guards against future settings-shape drift.
        assert_eq!(app_locale(), Locale::En);
    }
}
