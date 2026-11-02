use efi_agent_filesystem::{FileSystem, capture::Capture, namespace::Namespace, text};
use serde_json::Value;

#[test]
fn unicode_and_binary_ranges_reconstruct_large_files_at_asymmetric_boundaries() {
    let fs = Namespace::temporary(4 * 1024 * 1024, 128, 42, None);
    let expected = "ab中😀\n\"\\".repeat(60_001);
    text::save(&fs, "nested/code.txt", &expected).unwrap();
    text::append(&fs, "nested/code.txt", "tail中").unwrap();
    let expected = expected + "tail中";
    assert!(text::read(&fs, "nested/code.txt").is_err());
    for limit in [4, 7, 257, 65537] {
        let mut offset = 0;
        let mut actual = String::new();
        loop {
            let page: Value = serde_json::from_str(
                &text::read_range(&fs, "nested/code.txt", offset, limit, false).unwrap(),
            )
            .unwrap();
            actual.push_str(page["content"].as_str().unwrap());
            let next = page["next_offset"].as_u64().unwrap();
            assert!(next > offset);
            assert!(next - offset <= limit as u64);
            offset = next;
            if page["eof"] == true {
                break;
            }
        }
        assert_eq!(actual.as_bytes(), expected.as_bytes());
    }
    let page: Value =
        serde_json::from_str(&text::read_range(&fs, "nested/code.txt", 3, 7, true).unwrap())
            .unwrap();
    let hex = page["content"].as_str().unwrap();
    let actual: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(actual, expected.as_bytes()[3..10]);
    assert!(text::read_range(&fs, "nested/code.txt", 3, 7, false).is_err());
    assert!(text::read_range(&fs, "nested/code.txt", u64::MAX, 4, false).is_err());
}

#[test]
fn failed_atomic_replacement_preserves_original_and_cleans_staging() {
    let fs = Namespace::temporary(8192, 64, 42, None);
    text::save(&fs, "code", "original bytes").unwrap();
    fs.set_mode("code", 0o640).unwrap();
    assert!(text::save(&fs, "code", &"x".repeat(20_000)).is_err());
    assert_eq!(text::read(&fs, "code").unwrap(), "original bytes");
    assert_eq!(fs.metadata("code").unwrap().mode, 0o640);
    let entries = fs.read_directory("").unwrap();
    let names: Vec<_> = entries
        .iter()
        .filter(|(name, _)| !matches!(*name, "." | ".."))
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(names, vec!["code".to_string()]);
    text::save(&fs, "code", "new bytes").unwrap();
    assert_eq!(text::read(&fs, "code").unwrap(), "new bytes");
    assert_eq!(fs.metadata("code").unwrap().mode, 0o640);
    fs.symlink("code", "link").unwrap();
    assert!(text::save(&fs, "link", "wrong target").is_err());
    assert_eq!(text::read(&fs, "code").unwrap(), "new bytes");
    text::save(&fs, ".agent-save-0", "staging-name destination").unwrap();
    assert_eq!(
        text::read(&fs, ".agent-save-0").unwrap(),
        "staging-name destination"
    );
}

#[test]
fn output_logs_preserve_every_byte_and_preview_both_ends() {
    let fs = Namespace::temporary(4 * 1024 * 1024, 128, 42, None);
    let mut capture = Capture::new(&fs).unwrap();
    let stdout: Vec<_> = (0..700_013).map(|i| (i * 37 + 11) as u8).collect();
    let stderr = b"FINAL_TEST_FAILURE\n";
    for chunk in stdout.chunks(7919) {
        capture.append(chunk, &[]).unwrap();
    }
    capture.append(&[], stderr).unwrap();
    capture.flush().unwrap();
    let actual = text::read_bytes(&fs, &capture.stdout.path, stdout.len()).unwrap();
    assert_eq!(actual, stdout);
    assert_eq!(
        text::read_bytes(&fs, &capture.stderr.path, stderr.len()).unwrap(),
        stderr
    );
    let preview = capture.result("Exit code: 1");
    assert!(preview.contains("FINAL_TEST_FAILURE"));
    assert!(preview.contains("Output preview omits"));
    assert_eq!(capture.stdout.bytes, stdout.len() as u64);
    let second = Capture::new(&fs).unwrap();
    assert_ne!(second.stdout.path, capture.stdout.path);
}
