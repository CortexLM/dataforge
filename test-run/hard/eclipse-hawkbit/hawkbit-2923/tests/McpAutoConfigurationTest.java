package org.eclipse.hawkbit.mcp.server;

import org.eclipse.hawkbit.mcp.server.config.HawkbitMcpProperties;
import org.junit.jupiter.api.Test;
import org.springframework.boot.WebApplicationType;
import org.springframework.boot.builder.SpringApplicationBuilder;
import org.springframework.context.ConfigurableApplicationContext;
import org.springframework.context.annotation.Configuration;
import org.springframework.boot.autoconfigure.EnableAutoConfiguration;

import static org.assertj.core.api.Assertions.assertThat;

class McpAutoConfigurationTest {

    @Test
    void autoConfigurationProvidesPropertiesBeanWithDefaults() {
        try (ConfigurableApplicationContext context = new SpringApplicationBuilder(TestApp.class)
                .web(WebApplicationType.NONE)
                .properties(
                        "spring.main.banner-mode=off",
                        "hawkbit.mcp.mgmt-url=http://example.invalid:8080")
                .run()) {
            HawkbitMcpProperties properties = context.getBean(HawkbitMcpProperties.class);

            assertThat(properties.getMgmtUrl()).isEqualTo("http://example.invalid:8080");
            assertThat(properties.isToolsEnabled()).isTrue();
            assertThat(properties.isResourcesEnabled()).isTrue();
            assertThat(properties.isPromptsEnabled()).isTrue();
        }
    }

    @Test
    void autoConfigurationBindsNestedOperationDefaults() {
        try (ConfigurableApplicationContext context = new SpringApplicationBuilder(TestApp.class)
                .web(WebApplicationType.NONE)
                .properties(
                        "spring.main.banner-mode=off",
                        "hawkbit.mcp.mgmt-url=http://example.invalid:8081")
                .run()) {
            HawkbitMcpProperties properties = context.getBean(HawkbitMcpProperties.class);

            assertThat(properties.getOperations().isGlobalOperationEnabled("delete")).isTrue();
            assertThat(properties.getOperations().getActions().getOperationEnabled("delete-batch"))
                    .isTrue();
        }
    }

    @Configuration
    @EnableAutoConfiguration
    static class TestApp {
    }
}
