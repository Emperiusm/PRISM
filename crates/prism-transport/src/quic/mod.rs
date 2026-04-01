// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod config;
pub mod socket;
pub mod auth_token;
pub mod connection;

pub use config::{latency_transport_config, throughput_transport_config};
pub use socket::{create_latency_socket, create_throughput_socket, set_dscp};
pub use auth_token::{generate_throughput_token, validate_throughput_token};
pub use connection::QuicConnection;
