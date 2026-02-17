#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../../.." && pwd)"
BASE_SOURCE="$ROOT_DIR/hawkbit-mcp/src/main/java/org/eclipse/hawkbit/mcp/server/McpServerStart.java"
ALT_SOURCE="$ROOT_DIR/hawkbit-mcp/hawkbit-mcp-server/src/main/java/org/eclipse/hawkbit/mcp/server/McpServerStart.java"

if [[ -f "$ALT_SOURCE" ]]; then
  SOURCE_PATH="$ALT_SOURCE"
else
  SOURCE_PATH="$BASE_SOURCE"
fi

if [[ ! -f "$SOURCE_PATH" ]]; then
  echo "Cannot locate McpServerStart.java" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
SRC_ROOT="$TMP_DIR/src"
CLASS_ROOT="$TMP_DIR/classes"
mkdir -p "$SRC_ROOT" "$CLASS_ROOT"

write_stub() {
  local path="$1"
  local content="$2"
  mkdir -p "$(dirname "$SRC_ROOT/$path")"
  printf "%s\n" "$content" > "$SRC_ROOT/$path"
}

write_stub "org/springframework/boot/SpringApplication.java" $'package org.springframework.boot;\n\npublic class SpringApplication {\n    public static void run(Class<?> app, String[] args) {\n    }\n}\n'

write_stub "org/springframework/boot/autoconfigure/SpringBootApplication.java" $'package org.springframework.boot.autoconfigure;\n\nimport java.lang.annotation.ElementType;\nimport java.lang.annotation.Retention;\nimport java.lang.annotation.RetentionPolicy;\nimport java.lang.annotation.Target;\n\n@Retention(RetentionPolicy.RUNTIME)\n@Target(ElementType.TYPE)\npublic @interface SpringBootApplication {\n    Class<?>[] exclude() default {};\n}\n'

write_stub "org/springframework/boot/autoconfigure/security/servlet/UserDetailsServiceAutoConfiguration.java" $'package org.springframework.boot.autoconfigure.security.servlet;\n\npublic class UserDetailsServiceAutoConfiguration {}\n'

write_stub "org/springframework/boot/context/properties/EnableConfigurationProperties.java" $'package org.springframework.boot.context.properties;\n\nimport java.lang.annotation.ElementType;\nimport java.lang.annotation.Retention;\nimport java.lang.annotation.RetentionPolicy;\nimport java.lang.annotation.Target;\n\n@Retention(RetentionPolicy.RUNTIME)\n@Target(ElementType.TYPE)\npublic @interface EnableConfigurationProperties {\n    Class<?>[] value() default {};\n}\n'

write_stub "org/eclipse/hawkbit/mcp/server/config/HawkbitMcpProperties.java" $'package org.eclipse.hawkbit.mcp.server.config;\n\npublic class HawkbitMcpProperties {}\n'

TARGET_DIR="$SRC_ROOT/org/eclipse/hawkbit/mcp/server"
mkdir -p "$TARGET_DIR"
cp "$SOURCE_PATH" "$TARGET_DIR/McpServerStart.java"

cat > "$SRC_ROOT/AnnotationRunner.java" <<'RUNNER'
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
RUNNER

find "$SRC_ROOT" -name "*.java" > "$TMP_DIR/sources.txt"

javac -d "$CLASS_ROOT" @"$TMP_DIR/sources.txt"

java -cp "$CLASS_ROOT" AnnotationRunner

rm -rf "$TMP_DIR"
