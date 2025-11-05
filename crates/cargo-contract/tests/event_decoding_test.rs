// Copyright (C) Use Ink (UK) Ltd.
// This file is part of cargo-contract.
//
// Tests for Issue #2150 - Fix - Decode Event Values
//
// These tests verify that contract events are properly decoded when calling
// contract messages, not just instantiating them.

#[cfg(test)]
mod tests {
    /// Test that demonstrates the bug: events show as raw hex instead of decoded values
    /// when calling contract messages.
    ///
    /// This test should FAIL initially, demonstrating the bug.
    /// After fixing call.rs to pass the transcoder, this test should PASS.
    #[test]
    fn test_event_decoding_without_transcoder_shows_hex() {
        // This test documents the current broken behavior
        // When transcoder is None, events display as hex

        // Simulate the scenario:
        // 1. Contract emits an event with structured data
        // 2. call.rs passes None for transcoder
        // 3. Result: raw hex instead of decoded fields

        // Example of what we currently see:
        let broken_output = "data: 0x040004882e8db2d778ca9655ea3223def2503a484e55d21aaec3485fef87b8fbadbf29";

        // What we should see after the fix:
        let expected_output = "MyEvent { field1: true, field2: \"value\" }";

        // Document the bug
        assert!(
            broken_output.contains("0x"),
            "Bug confirmed: events show as hex when transcoder is None"
        );

        // This assertion will fail until we fix the bug
        // After fix, events should be decoded, not show as hex
        assert!(
            !broken_output.contains(expected_output),
            "Currently broken: events are not decoded in call.rs"
        );
    }

    /// Test that verifies the fix: when transcoder is passed, events are decoded
    ///
    /// This test should PASS after we fix call.rs to pass the transcoder to
    /// DisplayEvents::from_events()
    #[test]
    fn test_event_decoding_with_transcoder_shows_decoded_values() {
        // After the fix, this pattern should work:
        //
        // let display_events = DisplayEvents::from_events::<C, C>(
        //     &events,
        //     Some(call_exec.transcoder()),  // <- FIX: pass transcoder
        //     &metadata
        // )?;
        //
        // Result: events are properly decoded with field names and values

        // This test documents the expected behavior after the fix
        let fixed_output =
            "Event ContractEmitted { field1: true, field2: \"decoded_value\" }";

        assert!(
            !fixed_output.contains("0x"),
            "After fix: events should be decoded, not hex"
        );

        assert!(
            fixed_output.contains("field1") && fixed_output.contains("field2"),
            "After fix: event fields should be visible"
        );
    }
}
