//! Declarative-only content: validation precedes any persistence or activation.
use crate::experience::valid_id;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_MANIFEST_BYTES: usize = 128 * 1024;
pub const MAX_PACKS: usize = 16;
/// Stable template provenance. Presence is data, never package activation.
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ContentRef {
    pub package_id: String,
    pub package_version: u32,
    pub template_id: String,
}
impl ContentRef {
    pub fn validate(&self)->Result<(),String> {
        if !valid_id(&self.package_id) || self.package_version==0 || !namespaced(&self.template_id,&self.package_id) {
            return Err("pack_invalid_reference".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u8,
    pub id: String,
    pub version: u32,
    pub name: String,
    pub author: String,
    pub license: String,
    pub runtime_version: u8,
    pub dependencies: Vec<Dependency>,
    pub characters: Vec<Character>,
    pub objects: Vec<Object>,
    pub motions: Vec<Motion>,
    pub activities: Vec<Behavior>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dependency {
    pub id: String,
    pub minimum_version: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub rest_preference: u16,
    pub sociability: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Object {
    pub id: String,
    pub name: String,
    pub kind: ObjectKind,
    pub offers: Vec<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Rest,
    Snack,
    Meeting,
    Play,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Motion {
    pub id: String,
    pub capability: MotionCapability,
    pub fallback: MotionCapability,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionCapability {
    Idle,
    Walk,
    Wave,
    Play,
    Rest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Behavior {
    pub id: String,
    pub object_id: String,
    pub action: Action,
    pub condition: Condition,
    pub weight: u16,
    pub duration_ms: u32,
    pub motion_id: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Rest,
    Snack,
    Play,
    Socialize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    LowEnergy,
    LowNourishment,
    LowEngagement,
    LowSocial,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Installed {
    pub manifest: Manifest,
    pub revision: u64,
    pub active: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub manifest: Manifest,
    pub resources: Vec<String>,
    pub warnings: Vec<&'static str>,
}
fn label(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}
fn namespaced(id: &str, package: &str) -> bool {
    id.strip_prefix(package)
        .and_then(|s| s.strip_prefix(':'))
        .is_some_and(valid_id)
}
/// Resolve declarative provenance only. A saved reference never activates a
/// package or supplies native ownership, a lease, animation or action proof.
pub fn resolve_object<'a>(packs: &'a [Installed], reference: &ContentRef, kind: crate::world_state::AnchorKind) -> Option<&'a Object> {
    reference.validate().ok()?;
    validate_catalog(packs).ok()?;
    let package = packs.iter().find(|package| package.active
        && package.manifest.id == reference.package_id
        && package.manifest.version == reference.package_version)?;
    package.manifest.objects.iter().find(|object| object.id == reference.template_id
        && matches!((object.kind, kind),
            (ObjectKind::Rest, crate::world_state::AnchorKind::Rest)
            | (ObjectKind::Snack, crate::world_state::AnchorKind::Snack)
            | (ObjectKind::Meeting, crate::world_state::AnchorKind::Meeting)
            | (ObjectKind::Play, crate::world_state::AnchorKind::Play)))
}
impl Manifest {
    pub fn parse(json: &str) -> Result<Self, String> {
        if json.len() > MAX_MANIFEST_BYTES {
            return Err("pack_too_large".into());
        }
        let m: Self = serde_json::from_str(json).map_err(|_| "pack_invalid_schema")?;
        m.validate()?;
        Ok(m)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.runtime_version != 1 {
            return Err("pack_unsupported_version".into());
        }
        if !valid_id(&self.id)
            || self.version == 0
            || !label(&self.name, 80)
            || !label(&self.author, 120)
            || !label(&self.license, 80)
            || self.dependencies.len() > 8
            || self.characters.len() > 16
            || self.objects.len() > 32
            || self.motions.len() > 32
            || self.activities.len() > 32
        {
            return Err("pack_invalid_schema".into());
        }
        let ids: Vec<&String> = self
            .characters
            .iter()
            .map(|x| &x.id)
            .chain(self.objects.iter().map(|x| &x.id))
            .chain(self.motions.iter().map(|x| &x.id))
            .chain(self.activities.iter().map(|x| &x.id))
            .collect();
        if ids.iter().any(|id| !namespaced(id, &self.id))
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
        {
            return Err("pack_invalid_id".into());
        }
        if self
            .dependencies
            .iter()
            .any(|d| !valid_id(&d.id) || d.id == self.id || d.minimum_version == 0)
            || self
                .dependencies
                .iter()
                .map(|d| &d.id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.dependencies.len()
        {
            return Err("pack_invalid_dependency".into());
        }
        // No filesystem path, URL, script, executable or external model is accepted.
        let avatars = [
            "builtin:robot-expressive",
            "builtin:fox",
            "builtin:kaykit-knight",
            "builtin:kaykit-mage",
            "builtin:kaykit-rogue",
            "builtin:kaykit-skeleton",
        ];
        if self.characters.iter().any(|c| {
            !label(&c.name, 80)
                || !avatars.contains(&c.avatar.as_str())
                || c.rest_preference > 1000
                || c.sociability > 1000
        }) {
            return Err("pack_invalid_character".into());
        }
        for o in &self.objects {
            if !label(&o.name, 80)
                || o.offers.is_empty()
                || o.offers.len() > 8
                || o.offers.iter().collect::<BTreeSet<_>>().len() != o.offers.len()
                || o.offers.iter().any(|id| {
                    !self
                        .activities
                        .iter()
                        .any(|a| &a.id == id && a.object_id == o.id)
                })
            {
                return Err("pack_invalid_offer".into());
            }
        }
        for a in &self.activities {
            let Some(o) = self.objects.iter().find(|o| o.id == a.object_id) else {
                return Err("pack_invalid_reference".into());
            };
            let compatible = matches!(
                (o.kind, a.action),
                (ObjectKind::Rest, Action::Rest)
                    | (ObjectKind::Snack, Action::Snack)
                    | (ObjectKind::Play, Action::Play)
                    | (ObjectKind::Meeting, Action::Socialize)
            );
            if !compatible
                || !o.offers.contains(&a.id)
                || !self.motions.iter().any(|m| m.id == a.motion_id)
                || a.weight == 0
                || a.weight > 1000
                || !(500..=60_000).contains(&a.duration_ms)
            {
                return Err("pack_invalid_activity".into());
            }
        }
        Ok(())
    }
    pub fn preview(self) -> Preview {
        let resources = self
            .characters
            .iter()
            .map(|c| c.avatar.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Preview {
            manifest: self,
            resources,
            warnings: vec![
                "pack_declarative_only",
                "pack_activation_does_not_start_life",
            ],
        }
    }
}
pub fn validate_catalog(packs: &[Installed]) -> Result<(), String> {
    if packs.len() > MAX_PACKS {
        return Err("pack_capacity".into());
    }
    let ids = packs
        .iter()
        .map(|p| &p.manifest.id)
        .collect::<BTreeSet<_>>();
    if ids.len() != packs.len() {
        return Err("pack_invalid_id".into());
    }
    for pack in packs {
        pack.manifest.validate()?;
        if pack.active {
            for d in &pack.manifest.dependencies {
                if !packs.iter().any(|p| {
                    p.active && p.manifest.id == d.id && p.manifest.version >= d.minimum_version
                }) {
                    return Err("pack_missing_dependency".into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn example() -> String {
        include_str!("../../public/content-packs/desk-garden.json").into()
    }
    #[test]
    fn example_has_four_real_content_types() {
        let m = Manifest::parse(&example()).unwrap();
        assert_eq!(m.characters.len(), 1);
        assert_eq!(m.objects.len(), 1);
        assert_eq!(m.motions.len(), 1);
        assert_eq!(m.activities.len(), 1);
    }
    #[test]
    fn scripts_and_paths_rejected_before_install() {
        let mut v: serde_json::Value = serde_json::from_str(&example()).unwrap();
        v["script"] = serde_json::json!("run()");
        assert!(Manifest::parse(&v.to_string()).is_err());
        v.as_object_mut().unwrap().remove("script");
        v["characters"][0]["avatar"] = serde_json::json!("../../secret");
        assert!(Manifest::parse(&v.to_string()).is_err());
    }
    #[test]
    fn dangling_offer_and_migration_rejected() {
        let mut m = Manifest::parse(&example()).unwrap();
        m.activities[0].motion_id = "garden:missing".into();
        assert!(m.validate().is_err());
        m.schema_version = 2;
        assert_eq!(m.validate().unwrap_err(), "pack_unsupported_version");
    }
    #[test]
    fn dependency_cannot_be_removed_while_active() {
        let m = Manifest::parse(&example()).unwrap();
        let mut dependent = m.clone();
        dependent.id = "other".into();
        dependent.characters.clear();
        dependent.objects.clear();
        dependent.motions.clear();
        dependent.activities.clear();
        dependent.dependencies = vec![Dependency {
            id: m.id.clone(),
            minimum_version: 1,
        }];
        let base = Installed {
            manifest: m,
            revision: 1,
            active: false,
        };
        let other = Installed {
            manifest: dependent,
            revision: 1,
            active: true,
        };
        assert_eq!(
            validate_catalog(&[base, other]).unwrap_err(),
            "pack_missing_dependency"
        );
    }
    #[test]
    fn object_resolution_requires_active_exact_version_namespace_and_anchor_kind() {
        let manifest=Manifest::parse(&example()).unwrap();
        let reference=ContentRef { package_id:manifest.id.clone(),package_version:manifest.version,
            template_id:manifest.objects[0].id.clone() };
        let mut packs=vec![Installed { manifest,revision:1,active:true }];
        assert!(resolve_object(&packs,&reference,crate::world_state::AnchorKind::Play).is_some());
        for kind in [crate::world_state::AnchorKind::Rest,crate::world_state::AnchorKind::Snack,crate::world_state::AnchorKind::Meeting] {
            assert!(resolve_object(&packs,&reference,kind).is_none());
        }
        let mut changed=reference.clone();changed.package_version+=1;
        assert!(resolve_object(&packs,&changed,crate::world_state::AnchorKind::Play).is_none());
        changed=reference.clone();changed.template_id="desk-garden:missing".into();
        assert!(resolve_object(&packs,&changed,crate::world_state::AnchorKind::Play).is_none());
        changed=reference.clone();changed.template_id="other:seed".into();
        assert!(resolve_object(&packs,&changed,crate::world_state::AnchorKind::Play).is_none());
        packs[0].active=false;
        assert!(resolve_object(&packs,&reference,crate::world_state::AnchorKind::Play).is_none());
        packs[0].active=true;packs[0].manifest.version+=1;
        assert!(resolve_object(&packs,&reference,crate::world_state::AnchorKind::Play).is_none());
        assert!(resolve_object(&[],&reference,crate::world_state::AnchorKind::Play).is_none());
    }
    #[test]
    fn unresolved_dependencies_and_ambiguous_catalogues_never_supply_object_offers() {
        let manifest=Manifest::parse(&example()).unwrap();
        let reference=ContentRef { package_id:manifest.id.clone(),package_version:1,template_id:manifest.objects[0].id.clone() };
        let package=Installed { manifest,revision:1,active:true };
        assert!(resolve_object(&[package.clone(),package.clone()],&reference,crate::world_state::AnchorKind::Play).is_none());
        let mut dependent=package;
        dependent.manifest.dependencies.push(Dependency{id:"missing".into(),minimum_version:1});
        assert!(resolve_object(&[dependent],&reference,crate::world_state::AnchorKind::Play).is_none());
    }
}
