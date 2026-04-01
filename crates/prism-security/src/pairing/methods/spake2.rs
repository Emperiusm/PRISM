// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub fn generate_code() -> String {
    use rand::Rng;
    let code: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{:06}", code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_6_digits() {
        let code = generate_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
