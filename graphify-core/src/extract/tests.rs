// ponytail: allow missing errors doc as this is a unit test module with standard unwrap-ban-safe Result propagation
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::module_inception)]

#[cfg(test)]
mod tests {
    use crate::extract_file;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn run_extract(content: &str, ext: &str) -> anyhow::Result<crate::types::ExtractionResult> {
        let mut file = NamedTempFile::new()?;
        write!(file, "{content}")?;
        let path = file.into_temp_path();
        let target = path.with_extension(ext);
        std::fs::rename(&path, &target)?;
        let result = extract_file(&target);
        let _ = std::fs::remove_file(&target);
        result
    }

    // ---- Python ----

    #[test]
    fn test_extract_python_functions_and_classes() -> anyhow::Result<()> {
        let content = r#"
import os
from typing import List

class Greeter:
    def greet(self, name: str) -> str:
        return f"Hello {name}"

    def farewell(self) -> None:
        print("bye")

def helper() -> None:
    g = Greeter()
    g.greet("world")
"#;
        let result = run_extract(content, "py")?;
        assert_eq!(result.nodes[0].language, "python");
        assert!(result.nodes.iter().any(|n| n.kind == "class" && n.label == "Greeter"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "greet"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "farewell"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "helper"));
        assert!(result.edges.iter().any(|e| e.relation == "imports"));
        Ok(())
    }

    // ---- Rust ----

    #[test]
    fn test_extract_rust_struct_trait_and_function() -> anyhow::Result<()> {
        let content = r#"
use std::fmt;

struct Config {
    name: String,
}

trait Runner {
    fn run(&self) -> bool;
}

impl Runner for Config {
    fn run(&self) -> bool {
        true
    }
}

pub fn main() -> Result<()> {
    let c = Config { name: "test".into() };
    c.run();
    Ok(())
}
"#;
        let result = run_extract(content, "rs")?;
        assert_eq!(result.nodes[0].language, "rust");
        assert!(result.nodes.iter().any(|n| n.kind == "struct" && n.label == "Config"));
        assert!(result.nodes.iter().any(|n| n.kind == "trait" && n.label == "Runner"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "main"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "run"));
        assert!(result.edges.iter().any(|e| e.relation == "contains"));
        Ok(())
    }

    // ---- Go ----

    #[test]
    fn test_extract_go_functions_and_struct() -> anyhow::Result<()> {
        let content = r#"
package main

import "fmt"

type User struct {
    Name string
}

func greet(u User) string {
    return "Hello " + u.Name
}

func main() {
    u := User{Name: "world"}
    fmt.Println(greet(u))
}
"#;
        let result = run_extract(content, "go")?;
        assert_eq!(result.nodes[0].language, "go");
        assert!(result.nodes.iter().any(|n| n.kind == "struct" && n.label == "User"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "greet"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "main"));
        assert!(result.edges.iter().any(|e| e.relation == "imports"));
        Ok(())
    }

    // ---- JavaScript ----

    #[test]
    fn test_extract_javascript_functions_classes_and_variables() -> anyhow::Result<()> {
        let content = r#"
import { db } from "./db";

class Service {
    constructor() {
        this.ready = true;
    }
}

function fetch() {
    return db.query("SELECT 1");
}

function helper() {
    const x = 42;
    return x;
}
"#;
        let result = run_extract(content, "js")?;
        assert_eq!(result.nodes[0].language, "javascript");
        assert!(result.nodes.iter().any(|n| n.kind == "class" && n.label == "Service"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "fetch"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "helper"));
        assert!(result.edges.iter().any(|e| e.relation == "imports"));
        Ok(())
    }

    // ---- PHP ----

    #[test]
    fn test_extract_php_class_and_function() -> anyhow::Result<()> {
        let content = r#"
<?php
namespace App;

class Hello {
    public function greet(string $name): string {
        return "Hello " . $name;
    }
}
"#;
        let result = run_extract(content, "php")?;
        for n in &result.nodes {
            println!("Node: kind={}, label={}", n.kind, n.label);
        }
        assert_eq!(result.nodes[0].language, "php");
        assert!(result.nodes.iter().any(|n| n.kind == "class" && n.label == "Hello"));
        assert!(result.nodes.iter().any(|n| n.kind == "method" && n.label == "greet"));
        Ok(())
    }

    // ---- C ----

    #[test]
    fn test_extract_c_struct_and_function() -> anyhow::Result<()> {
        let content = r"
#include <stdio.h>

struct Point {
    int x;
    int y;
};

int add(int a, int b) {
    return a + b;
}

void main() {
    struct Point p;
    p.x = add(1, 2);
}
";
        let result = run_extract(content, "c")?;
        assert_eq!(result.nodes[0].language, "c");
        assert!(result.nodes.iter().any(|n| n.kind == "struct" && n.label == "Point"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "add"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "main"));
        Ok(())
    }

    // ---- C++ ----

    #[test]
    fn test_extract_cpp_struct_and_function() -> anyhow::Result<()> {
        let content = r"
#include <vector>

struct Vec2 {
    float x;
    float y;
};

float length(Vec2 v) {
    return v.x * v.x + v.y * v.y;
}

int main() {
    Vec2 v{1.0, 2.0};
    auto len = length(v);
    return 0;
}
";
        let result = run_extract(content, "cpp")?;
        assert_eq!(result.nodes[0].language, "cpp");
        assert!(result.nodes.iter().any(|n| n.kind == "struct" && n.label == "Vec2"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "length"));
        assert!(result.nodes.iter().any(|n| n.kind == "function" && n.label == "main"));
        Ok(())
    }

    // ---- Java (existing) ----

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

    // ---- Swift (existing) ----

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
