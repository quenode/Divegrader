use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GradeConfig {
    pub min_avg_red: f32,
    pub max_hue_shift: f32,
    pub blue_magic: f32,
    pub sample_seconds: f32,
    pub threshold_ratio: f32,
    pub strength: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub gamma: f32,
    pub temperature: f32,
    pub preset: String,
    pub water: String,
    pub depth: String,
    pub profile: String,
}

impl Default for GradeConfig {
    fn default() -> Self {
        Self {
            min_avg_red: 60.0,
            max_hue_shift: 120.0,
            blue_magic: 1.2,
            sample_seconds: 2.0,
            threshold_ratio: 2000.0,
            strength: 1.0,
            contrast: 1.0,
            saturation: 1.0,
            gamma: 1.0,
            temperature: 0.0,
            preset: "default".to_string(),
            water: "auto".to_string(),
            depth: "auto".to_string(),
            profile: "default".to_string(),
        }
    }
}

impl GradeConfig {
    pub fn apply_preset(&mut self) {
        match self.preset.as_str() {
            "shallow" => {
                self.min_avg_red = 50.0;
                self.blue_magic = 1.1;
            }
            "deep" => {
                self.min_avg_red = 70.0;
                self.blue_magic = 1.3;
            }
            "green-water" => {
                self.min_avg_red = 65.0;
                self.blue_magic = 1.0;
            }
            "red-boost" => {
                self.min_avg_red = 80.0;
                self.blue_magic = 1.4;
            }
            _ => {}
        }
    }

    pub fn apply_profile(&mut self) {
        match self.profile.as_str() {
            "reef" => {
                self.water = "blue".to_string();
                self.depth = "auto".to_string();
            }
            "wreck" => {
                self.water = "green".to_string();
                self.depth = "mid".to_string();
            }
            "cave" => {
                self.water = "blue".to_string();
                self.depth = "deep".to_string();
                self.contrast = 1.05;
            }
            "night" => {
                self.min_avg_red = 50.0;
                self.blue_magic = 1.0;
                self.contrast = 1.1;
                self.saturation = 1.1;
            }
            _ => {}
        }
    }

    pub fn to_toml(&self) -> anyhow::Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn from_toml(s: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(s)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip() {
        let cfg = GradeConfig::default();
        let s = cfg.to_toml().unwrap();
        let back = GradeConfig::from_toml(&s).unwrap();
        assert_eq!(back.min_avg_red, cfg.min_avg_red);
        assert_eq!(back.water, cfg.water);
    }

    #[test]
    fn profile_night_sets_values() {
        let mut cfg = GradeConfig::default();
        cfg.profile = "night".to_string();
        cfg.apply_profile();
        assert_eq!(cfg.min_avg_red, 50.0);
        assert_eq!(cfg.saturation, 1.1);
    }
}