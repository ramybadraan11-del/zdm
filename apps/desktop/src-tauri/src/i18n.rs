//! ZDM Translations (Arabic / English / French)
//! Developed by Ramy Badraan
//!
//! The UI carries its own dictionary (TypeScript, so it can switch instantly
//! without a round-trip), but native strings — tray menu, desktop entries and
//! error messages produced in Rust — live here.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Ar,
    En,
    Fr,
}

impl Lang {
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_lowercase().as_str() {
            "ar" | "arabic" => Some(Self::Ar),
            "en" | "english" => Some(Self::En),
            "fr" | "french" | "français" => Some(Self::Fr),
            _ => None,
        }
    }

    /// Best effort detection: explicit setting first, then the OS locale.
    pub fn detect(setting: &str) -> Self {
        if let Some(lang) = Self::from_code(setting) {
            return lang;
        }
        for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Ok(value) = std::env::var(var) {
                let code = value.split(['_', '.', '@']).next().unwrap_or("");
                if let Some(lang) = Self::from_code(code) {
                    return lang;
                }
            }
        }
        Self::En
    }

    #[allow(dead_code)]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Ar => "ar",
            Self::En => "en",
            Self::Fr => "fr",
        }
    }

    #[allow(dead_code)]
    pub fn is_rtl(&self) -> bool {
        matches!(self, Self::Ar)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TrayStrings {
    pub tooltip: String,
    pub show_transfers: String,
    pub show_hide: String,
    pub add_url: String,
    pub download_video: String,
    pub pause_all: String,
    pub settings: String,
    pub about: String,
    pub quit: String,
}

pub fn tray_strings(lang: Lang) -> TrayStrings {
    match lang {
        Lang::Ar => TrayStrings {
            tooltip: "Zyzoom Download Manager — مدير التنزيل فائق السرعة".into(),
            show_transfers: "إظهار نوافذ التنزيل".into(),
            show_hide: "إظهار / إخفاء النافذة".into(),
            add_url: "إضافة رابط تنزيل".into(),
            download_video: "تنزيل فيديو".into(),
            pause_all: "إيقاف كل التنزيلات مؤقتاً".into(),
            settings: "الإعدادات".into(),
            about: "حول البرنامج".into(),
            quit: "خروج".into(),
        },
        Lang::Fr => TrayStrings {
            tooltip: "Zyzoom Download Manager — accélérateur de téléchargement".into(),
            show_transfers: "Afficher les fenêtres de transfert".into(),
            show_hide: "Afficher / masquer la fenêtre".into(),
            add_url: "Ajouter un lien".into(),
            download_video: "Télécharger une vidéo".into(),
            pause_all: "Suspendre tous les téléchargements".into(),
            settings: "Paramètres".into(),
            about: "À propos".into(),
            quit: "Quitter".into(),
        },
        Lang::En => TrayStrings {
            tooltip: "Zyzoom Download Manager — turbo download accelerator".into(),
            show_transfers: "Show transfer windows".into(),
            show_hide: "Show / hide window".into(),
            add_url: "Add download link".into(),
            download_video: "Download video".into(),
            pause_all: "Pause all downloads".into(),
            settings: "Settings".into(),
            about: "About".into(),
            quit: "Quit".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_all_supported_languages() {
        assert_eq!(Lang::detect("ar"), Lang::Ar);
        assert_eq!(Lang::detect("en"), Lang::En);
        assert_eq!(Lang::detect("fr"), Lang::Fr);
    }

    #[test]
    fn unknown_falls_back_to_environment_or_english() {
        assert_eq!(Lang::from_code("de"), None);
        assert!(Lang::Ar.is_rtl());
        assert!(!Lang::Fr.is_rtl());
    }

    #[test]
    fn every_language_has_tray_strings() {
        for lang in [Lang::Ar, Lang::En, Lang::Fr] {
            let s = tray_strings(lang);
            assert!(!s.quit.is_empty());
            assert!(!s.tooltip.is_empty());
        }
    }
}
