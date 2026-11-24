//! Manifest → IPC `ModuleMeta`.

use anm_types::{ModuleAuthor, ModuleMeta, ModulePermissions};

use crate::model::Manifest;

impl Manifest {
    pub fn to_meta(
        &self,
        enabled: bool,
        trusted: bool,
        installed_at: &str,
        path: &str,
    ) -> ModuleMeta {
        ModuleMeta {
            id: self.id.clone(),
            kind: self.kind,
            name: self.name.clone(),
            version: self.version.clone(),
            api_version: self.api_version,
            author: ModuleAuthor {
                name: self.author.name.clone(),
                contact: self.author.contact.clone(),
                public_key: self.author.public_key.clone(),
            },
            released_at: self.released_at.clone(),
            permissions: ModulePermissions {
                network: self.permissions.network.clone(),
                storage: self.permissions.storage,
            },
            enabled,
            trusted,
            installed_at: installed_at.to_string(),
            path: path.to_string(),
            built_in: false,
            description: self.description.clone(),
            qualities: self.qualities.clone(),
            protocols: self.protocols.clone(),
            languages: self.languages.clone(),
            dependencies: self
                .dependencies
                .iter()
                .map(|d| anm_types::ModuleDependency {
                    id: d.id.clone(),
                    requirement: d.requirement.clone(),
                })
                .collect(),
            app_compatibility: self.app_compatibility.as_ref().map(|app| {
                anm_types::ModuleAppCompatibility {
                    min_app_version: app.min_app_version.clone(),
                    max_app_version: app.max_app_version.clone(),
                }
            }),
        }
    }
}
