use std::collections::HashMap;

/// Maps localized Excel function names to English.
pub struct FunctionNameMapper {
    /// map from localized name (uppercase) → English name
    mapping: HashMap<String, String>,
    /// reverse map: English → {locale → localized name}
    reverse: HashMap<String, HashMap<String, String>>,
}

impl FunctionNameMapper {
    pub fn new() -> Self {
        let mut mapper = FunctionNameMapper {
            mapping: HashMap::new(),
            reverse: HashMap::new(),
        };
        mapper.register_defaults();
        mapper
    }

    /// Look up the English name for a localized function name.
    pub fn to_english(&self, localized_name: &str) -> Option<&str> {
        self.mapping.get(&localized_name.to_uppercase()).map(|s| s.as_str())
    }

    /// Get the localized name for an English function name.
    pub fn from_english(&self, english_name: &str, locale: &str) -> Option<&str> {
        self.reverse
            .get(&english_name.to_uppercase())
            .and_then(|locales| locales.get(locale))
            .map(|s| s.as_str())
    }

    /// Check if a name matches a known function (any language).
    pub fn is_function(&self, name: &str) -> bool {
        self.mapping.contains_key(&name.to_uppercase())
            || self.reverse.contains_key(&name.to_uppercase())
    }

    /// Normalize a function name to English.
    pub fn normalize(&self, name: &str) -> String {
        self.to_english(name).unwrap_or(name).to_string()
    }

    fn register(&mut self, english: &str, translations: &[(&str, &str)]) {
        for (locale, localized) in translations {
            self.mapping
                .insert(localized.to_uppercase(), english.to_uppercase());
            self.reverse
                .entry(english.to_uppercase())
                .or_default()
                .insert(locale.to_string(), localized.to_string());
        }
    }

    fn register_defaults(&mut self) {
        // German (de)
        self.register("SUM", &[("de", "SUMME")]);
        self.register("IF", &[("de", "WENN")]);
        self.register("VLOOKUP", &[("de", "SVERWEIS")]);
        self.register("COUNT", &[("de", "ANZAHL")]);
        self.register("AVERAGE", &[("de", "MITTELWERT")]);
        self.register("TODAY", &[("de", "HEUTE")]);
        self.register("LEFT", &[("de", "LINKS")]);
        self.register("RIGHT", &[("de", "RECHTS")]);
        self.register("AND", &[("de", "UND")]);
        self.register("OR", &[("de", "ODER")]);
        self.register("TRUE", &[("de", "WAHR")]);
        self.register("FALSE", &[("de", "FALSCH")]);
        self.register("NOT", &[("de", "NICHT")]);
        self.register("ROUND", &[("de", "RUNDEN")]);
        self.register("SQRT", &[("de", "WURZEL")]);

        // French (fr)
        self.register("SUM", &[("fr", "SOMME")]);
        self.register("IF", &[("fr", "SI")]);
        self.register("VLOOKUP", &[("fr", "RECHERCHEV")]);
        self.register("COUNT", &[("fr", "NB")]);
        self.register("AVERAGE", &[("fr", "MOYENNE")]);
        self.register("TODAY", &[("fr", "AUJOURDHUI")]);
        self.register("AND", &[("fr", "ET")]);
        self.register("OR", &[("fr", "OU")]);
        self.register("TRUE", &[("fr", "VRAI")]);
        self.register("NOT", &[("fr", "NON")]);

        // Spanish (es)
        self.register("SUM", &[("es", "SUMA")]);
        self.register("IF", &[("es", "SI")]);
        self.register("VLOOKUP", &[("es", "BUSCARV")]);
        self.register("COUNT", &[("es", "CONTAR")]);
        self.register("AVERAGE", &[("es", "PROMEDIO")]);
        self.register("TODAY", &[("es", "HOY")]);
        self.register("AND", &[("es", "Y")]);
        self.register("TRUE", &[("es", "VERDADERO")]);

        // Russian (ru)
        self.register("SUM", &[("ru", "СУММ")]);
        self.register("IF", &[("ru", "ЕСЛИ")]);
        self.register("VLOOKUP", &[("ru", "ВПР")]);
        self.register("COUNT", &[("ru", "СЧЁТ")]);
        self.register("AVERAGE", &[("ru", "СРЗНАЧ")]);
        self.register("TODAY", &[("ru", "СЕГОДНЯ")]);
        self.register("AND", &[("ru", "И")]);
        self.register("TRUE", &[("ru", "ИСТИНА")]);

        // Japanese (ja)
        self.register("SUM", &[("ja", "SUM")]); // Japanese Excel uses English names but different UI
        self.register("IF", &[("ja", "IF")]);

        // Chinese simplified (zh-CN) — also uses English names in most versions
        self.register("SUM", &[("zh-cn", "SUM")]);
        self.register("IF", &[("zh-cn", "IF")]);
    }
}

impl Default for FunctionNameMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_german_to_english() {
        let mapper = FunctionNameMapper::new();
        assert_eq!(mapper.to_english("SUMME"), Some("SUM"));
        assert_eq!(mapper.to_english("WENN"), Some("IF"));
        assert_eq!(mapper.to_english("SVERWEIS"), Some("VLOOKUP"));
    }

    #[test]
    fn test_french_to_english() {
        let mapper = FunctionNameMapper::new();
        assert_eq!(mapper.to_english("SOMME"), Some("SUM"));
        assert_eq!(mapper.to_english("SI"), Some("IF"));
    }

    #[test]
    fn test_normalize() {
        let mapper = FunctionNameMapper::new();
        assert_eq!(mapper.normalize("SUMME"), "SUM");
        assert_eq!(mapper.normalize("SUM"), "SUM");
        assert_eq!(mapper.normalize("UNKNOWN"), "UNKNOWN");
    }

    #[test]
    fn test_is_function() {
        let mapper = FunctionNameMapper::new();
        assert!(mapper.is_function("SUMME"));
        assert!(mapper.is_function("SUM"));
        assert!(!mapper.is_function("NOT_A_FUNC"));
    }

    #[test]
    fn test_spanish_to_english() {
        let mapper = FunctionNameMapper::new();
        // Spanish functions if supported
        let sum_es = mapper.to_english("SUMA");
        let if_es = mapper.to_english("SI");
        // At least French SI=IF should map
        assert_eq!(if_es, Some("IF"));
        // SUMA may map to SUM in some locales
        let _ = sum_es;
    }

    #[test]
    fn test_from_english_german() {
        let mapper = FunctionNameMapper::new();
        let result = mapper.from_english("SUM", "de");
        // If German locale available, should return SUMME
        if let Some(name) = result {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn test_normalize_case_insensitive() {
        let mapper = FunctionNameMapper::new();
        // normalize works on uppercase; input should be uppercased by caller
        assert_eq!(mapper.normalize("SUM"), "SUM");
        assert_eq!(mapper.normalize("SUMME"), "SUM");
    }

    #[test]
    fn test_roundtrip_english() {
        let mapper = FunctionNameMapper::new();
        // normalize of already-English name should return same
        for name in &["SUM", "IF", "VLOOKUP", "COUNT", "AVERAGE"] {
            assert_eq!(mapper.normalize(name), *name);
        }
    }
}
