import os
import subprocess
import tempfile
import unittest
from pathlib import Path


class McpServerStartAnnotationTest(unittest.TestCase):
    def test_enable_configuration_properties_removed(self):
        repo_root = Path(__file__).resolve().parents[3]
        base_source = repo_root / "hawkbit-mcp" / "src" / "main" / "java" / "org" / "eclipse" / "hawkbit" / "mcp" / "server" / "McpServerStart.java"
        alt_source = repo_root / "hawkbit-mcp" / "hawkbit-mcp-server" / "src" / "main" / "java" / "org" / "eclipse" / "hawkbit" / "mcp" / "server" / "McpServerStart.java"
        source_path = alt_source if alt_source.exists() else base_source
        self.assertTrue(source_path.exists())

        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_root = Path(tmp_dir)
            src_root = tmp_root / "src"
            class_root = tmp_root / "classes"
            src_root.mkdir(parents=True)
            class_root.mkdir(parents=True)

            self._write_stub_sources(src_root)

            target_path = src_root / "org" / "eclipse" / "hawkbit" / "mcp" / "server" / "McpServerStart.java"
            target_path.parent.mkdir(parents=True, exist_ok=True)
            target_path.write_text(source_path.read_text(), encoding="utf-8")

            runner_path = src_root / "AnnotationRunner.java"
            runner_path.write_text(self._runner_source(), encoding="utf-8")

            java_files = [str(p) for p in src_root.rglob("*.java")]
            subprocess.run(
                ["javac", "-d", str(class_root)] + java_files,
                check=True,
                capture_output=True,
            )

            result = subprocess.run(
                ["java", "-cp", str(class_root), "AnnotationRunner"],
                check=False,
                capture_output=True,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)

    def _write_stub_sources(self, src_root: Path) -> None:
        stub_sources = {
            "org/springframework/boot/SpringApplication.java": """
package org.springframework.boot;

public class SpringApplication {
    public static void run(Class<?> app, String[] args) {
    }
}
""",
            "org/springframework/boot/autoconfigure/SpringBootApplication.java": """
package org.springframework.boot.autoconfigure;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface SpringBootApplication {
    Class<?>[] exclude() default {};
}
""",
            "org/springframework/boot/autoconfigure/security/servlet/UserDetailsServiceAutoConfiguration.java": """
package org.springframework.boot.autoconfigure.security.servlet;

public class UserDetailsServiceAutoConfiguration {}
""",
            "org/springframework/boot/context/properties/EnableConfigurationProperties.java": """
package org.springframework.boot.context.properties;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface EnableConfigurationProperties {
    Class<?>[] value() default {};
}
""",
            "org/eclipse/hawkbit/mcp/server/config/HawkbitMcpProperties.java": """
package org.eclipse.hawkbit.mcp.server.config;

public class HawkbitMcpProperties {}
""",
        }

        for relative, content in stub_sources.items():
            path = src_root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content.strip() + "\n", encoding="utf-8")

    @staticmethod
    def _runner_source() -> str:
        return """
import java.lang.annotation.Annotation;

public class AnnotationRunner {
    public static void main(String[] args) throws Exception {
        Class<?> target = Class.forName("org.eclipse.hawkbit.mcp.server.McpServerStart");
        boolean hasSpringBoot = target.isAnnotationPresent(
                org.springframework.boot.autoconfigure.SpringBootApplication.class);
        boolean hasEnableProperties = target.isAnnotationPresent(
                org.springframework.boot.context.properties.EnableConfigurationProperties.class);

        if (!hasSpringBoot) {
            System.err.println("Missing SpringBootApplication annotation");
            System.exit(2);
        }
        if (hasEnableProperties) {
            System.err.println("EnableConfigurationProperties should be removed");
            System.exit(3);
        }
    }
}
"""


if __name__ == "__main__":
    unittest.main()
