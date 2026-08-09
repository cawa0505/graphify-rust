// ponytail: allow missing errors doc as this is a unit test module with standard unwrap-ban-safe Result propagation
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::module_inception)]

#[cfg(test)]
mod tests {
    use crate::extract_file;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_extract_java_class_and_methods() -> anyhow::Result<()> {
        let content = r#"
            package com.example;
            import java.util.List;

            public class Calculator {
                private int result = 0;

                public Calculator() {
                    this.result = 1;
                }

                public int add(int a, int b) {
                    int sum = a + b;
                    logInfo();
                    return sum;
                }

                private void logInfo() {
                    System.out.println("Result: " + result);
                }
            }
        "#;

        let mut file = NamedTempFile::new()?;
        write!(file, "{}", content)?;
        let path = file.into_temp_path();
        // Rename tempfile to end with .java
        let java_path = path.with_extension("java");
        std::fs::rename(&path, &java_path)?;

        let result = extract_file(&java_path)?;

        // Assertions
        assert_eq!(result.nodes[0].language, "java");
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.kind == "class" && n.label == "Calculator")
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.kind == "function" && n.label == "add")
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.kind == "function" && n.label == "logInfo")
        );

        // Check for contains edges
        assert!(result.edges.iter().any(|e| e.relation == "contains"));

        // Cleanup
        let _ = std::fs::remove_file(java_path);
        Ok(())
    }

    #[test]
    fn test_extract_swift_class_and_methods() -> anyhow::Result<()> {
        let content = r#"
            import Foundation

            class UserProfile {
                var name: String

                init(name: String) {
                    self.name = name;
                    setupProfile()
                }

                func setupProfile() {
                    print("Setting up \(name)")
                }
            }
        "#;

        let mut file = NamedTempFile::new()?;
        write!(file, "{}", content)?;
        let path = file.into_temp_path();
        // Rename tempfile to end with .swift
        let swift_path = path.with_extension("swift");
        std::fs::rename(&path, &swift_path)?;

        let result = extract_file(&swift_path)?;

        // Print nodes for debugging
        for n in &result.nodes {
            println!(
                "Node: id={}, label={}, kind={}, language={}",
                n.id.0, n.label, n.kind, n.language
            );
        }

        // Assertions
        assert_eq!(result.nodes[0].language, "swift");
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.kind == "class" && n.label == "UserProfile")
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.kind == "function" && n.label == "init")
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.kind == "function" && n.label == "setupProfile")
        );

        // Cleanup
        let _ = std::fs::remove_file(swift_path);
        Ok(())
    }
}
