use similar::{ChangeTag, TextDiff};
use std::path::Path;

/// Prints a unified diff format showing the differences between old and new content
pub fn print_unified_diff(file_path: &Path, old_content: &str, new_content: &str) {
    let diff = TextDiff::from_lines(old_content, new_content);

    println!("--- {}", file_path.display());
    println!("+++ {}", file_path.display());

    for group in diff.grouped_ops(3) {
        let (mut old_count, mut new_count) = (0, 0);
        let first_op = &group[0];
        let old_start = match first_op {
            similar::DiffOp::Delete { old_index, .. }
            | similar::DiffOp::Replace { old_index, .. }
            | similar::DiffOp::Equal { old_index, .. } => *old_index,
            similar::DiffOp::Insert { .. } => 0,
        };
        let new_start = match first_op {
            similar::DiffOp::Insert { new_index, .. }
            | similar::DiffOp::Replace { new_index, .. }
            | similar::DiffOp::Equal { new_index, .. } => *new_index,
            similar::DiffOp::Delete { .. } => 0,
        };

        for op in group.iter() {
            match op {
                similar::DiffOp::Delete { old_len, .. } => old_count += old_len,
                similar::DiffOp::Insert { new_len, .. } => new_count += new_len,
                similar::DiffOp::Replace {
                    old_len, new_len, ..
                } => {
                    old_count += old_len;
                    new_count += new_len;
                }
                similar::DiffOp::Equal { len, .. } => {
                    old_count += len;
                    new_count += len;
                }
            }
        }

        // Print hunk header
        println!(
            "@@ -{},{} +{},{} @@",
            old_start + 1,
            old_count,
            new_start + 1,
            new_count
        );

        // Print each line with a prefix, using iter_changes for line-based diffs
        for op in group {
            for change in diff.iter_changes(&op) {
                match change.tag() {
                    ChangeTag::Delete => print!("-{}", change.value()),
                    ChangeTag::Insert => print!("+{}", change.value()),
                    ChangeTag::Equal => print!(" {}", change.value()),
                }
            }
        }
    }
}

/// Prints a side-by-side diff showing only the changed lines
pub fn print_side_by_side_diff(file_path: &Path, old_content: &str, new_content: &str) {
    println!("In file: {}", file_path.display());
    println!("(Side-by-side diff: only showing changed lines)\n");

    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    let max_len = old_lines.len().max(new_lines.len());

    for i in 0..max_len {
        let old_line = old_lines.get(i).unwrap_or(&"");
        let new_line = new_lines.get(i).unwrap_or(&"");

        if old_line != new_line {
            let line_num = i + 1; // 1-based line numbering
            println!("Line {}:", line_num);
            println!("  OLD: {}", old_line);
            println!("  NEW: {}", new_line);
            println!();
        }
    }
}
