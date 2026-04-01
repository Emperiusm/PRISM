// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Service mode detection and configuration.
//! When --service flag is passed, the server should run as a Windows service
//! instead of a console application.

/// Detect if we're running as a service.
pub fn is_service_mode() -> bool {
    std::env::args().any(|a| a == "--service")
}

/// Service install/uninstall commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceCommand {
    Install { binary_path: std::path::PathBuf },
    Uninstall,
    Start,
    Stop,
    Status,
}

impl ServiceCommand {
    /// Parse from CLI args.
    pub fn from_args() -> Option<Self> {
        let args: Vec<String> = std::env::args().collect();
        if args.contains(&"--install-service".to_string()) {
            let binary = std::env::current_exe().unwrap_or_default();
            Some(ServiceCommand::Install { binary_path: binary })
        } else if args.contains(&"--uninstall-service".to_string()) {
            Some(ServiceCommand::Uninstall)
        } else if args.contains(&"--start-service".to_string()) {
            Some(ServiceCommand::Start)
        } else if args.contains(&"--stop-service".to_string()) {
            Some(ServiceCommand::Stop)
        } else if args.contains(&"--service-status".to_string()) {
            Some(ServiceCommand::Status)
        } else {
            None
        }
    }
}

/// Generate the `sc create` command for Windows service registration.
pub fn sc_create_command(binary_path: &std::path::Path) -> String {
    format!(
        "sc create PRISMServer binPath= \"{}\" start= auto DisplayName= \"PRISM Remote Desktop Server\"",
        binary_path.display()
    )
}

/// Generate the `sc delete` command for uninstall.
pub fn sc_delete_command() -> String {
    "sc delete PRISMServer".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn is_not_service_mode() {
        // In test context, no --service flag is present.
        assert!(!is_service_mode());
    }

    #[test]
    fn sc_create_command_format() {
        let path = Path::new("C:\\Program Files\\PRISM\\prism-server.exe");
        let cmd = sc_create_command(path);
        assert!(cmd.contains("sc create PRISMServer"), "expected 'sc create PRISMServer' in: {cmd}");
        assert!(cmd.contains("prism-server.exe"), "expected binary path in: {cmd}");
    }

    #[test]
    fn sc_delete_command_format() {
        let cmd = sc_delete_command();
        assert!(cmd.contains("sc delete"), "expected 'sc delete' in: {cmd}");
        assert!(cmd.contains("PRISMServer"), "expected 'PRISMServer' in: {cmd}");
    }

    #[test]
    fn service_command_none_by_default() {
        // In test context, none of the service flags are present.
        assert_eq!(ServiceCommand::from_args(), None);
    }
}
