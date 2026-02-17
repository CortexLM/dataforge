/*
 * Copyright 2025 Haulmont.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package io.jmix.multitenancyflowui.impl;

import io.jmix.security.model.BaseRole;
import io.jmix.security.model.RoleSource;
import org.junit.jupiter.api.Test;
import org.springframework.security.core.GrantedAuthority;

import java.util.Collection;
import java.util.Collections;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Unit tests for SameTenantRoleHierarchyCandidatePredicate.
 * These tests verify the fix for NPE when currentRole is null during role creation.
 */
public class SameTenantRoleHierarchyCandidatePredicateUnitTest {

    @Test
    void testNullCurrentRoleWithAnnotatedClassSource() {
        SameTenantRoleHierarchyCandidatePredicate predicate = new SameTenantRoleHierarchyCandidatePredicate();
        
        // Annotated class roles should always be allowed regardless of current role being null
        BaseRole baseRole = createMockRole(RoleSource.ANNOTATED_CLASS, "annotatedRole", null);
        
        // When currentRole is null (role creation process), annotated class should return true
        // This should NOT throw NullPointerException (the bug fix)
        boolean result = predicate.test(null, baseRole);
        
        assertTrue(result, "Annotated class source should always return true even with null current role");
    }

    @Test
    void testNullCurrentRoleWithDatabaseSource() {
        SameTenantRoleHierarchyCandidatePredicate predicate = new SameTenantRoleHierarchyCandidatePredicate();
        
        // Database role with currentRole null should return false
        // (since we can't determine tenant without a current user context in this unit test)
        BaseRole baseRole = createMockRole(RoleSource.DATABASE, "databaseRole", "testTenant");
        
        // When currentRole is null, the old code would throw NPE
        // The fix should handle this gracefully
        boolean result = predicate.test(null, baseRole);
        
        // Without tenant provider, it should return false (currentRole is null)
        assertFalse(result, "Should return false when currentRole is null for database source");
    }

    @Test
    void testNullBaseRoleCandidate() {
        SameTenantRoleHierarchyCandidatePredicate predicate = new SameTenantRoleHierarchyCandidatePredicate();
        
        // Null base role candidate should return false
        BaseRole currentRole = createMockRole(RoleSource.DATABASE, "currentRole", "testTenant");
        
        boolean result = predicate.test(currentRole, null);
        
        assertFalse(result, "Null base role candidate should return false");
    }

    @Test
    void testCurrentRoleWithSameTenant() {
        SameTenantRoleHierarchyCandidatePredicate predicate = new SameTenantRoleHierarchyCandidatePredicate();
        
        // Both roles with same tenant should be allowed
        BaseRole currentRole = createMockRole(RoleSource.DATABASE, "currentRole", "tenantA");
        BaseRole baseRole = createMockRole(RoleSource.DATABASE, "baseRole", "tenantA");
        
        boolean result = predicate.test(currentRole, baseRole);
        
        assertTrue(result, "Roles with same tenant should be allowed");
    }

    @Test
    void testCurrentRoleWithDifferentTenant() {
        SameTenantRoleHierarchyCandidatePredicate predicate = new SameTenantRoleHierarchyCandidatePredicate();
        
        // Roles with different tenants should not be allowed
        BaseRole currentRole = createMockRole(RoleSource.DATABASE, "currentRole", "tenantA");
        BaseRole baseRole = createMockRole(RoleSource.DATABASE, "baseRole", "tenantB");
        
        boolean result = predicate.test(currentRole, baseRole);
        
        assertFalse(result, "Roles with different tenants should not be allowed");
    }

    @Test
    void testBothRolesWithNullTenant() {
        SameTenantRoleHierarchyCandidatePredicate predicate = new SameTenantRoleHierarchyCandidatePredicate();
        
        // Both roles with null tenant should be allowed
        BaseRole currentRole = createMockRole(RoleSource.DATABASE, "currentRole", null);
        BaseRole baseRole = createMockRole(RoleSource.DATABASE, "baseRole", null);
        
        boolean result = predicate.test(currentRole, baseRole);
        
        assertTrue(result, "Both roles with null tenant should be allowed");
    }

    @Test
    void testAnnotatedClassRoleAlwaysAllowed() {
        SameTenantRoleHierarchyCandidatePredicate predicate = new SameTenantRoleHierarchyCandidatePredicate();
        
        // Design-time roles (annotated class) should always be allowed as base roles
        // regardless of tenant matching
        BaseRole currentRole = createMockRole(RoleSource.DATABASE, "currentRole", "tenantA");
        BaseRole baseRole = createMockRole(RoleSource.ANNOTATED_CLASS, "designTimeRole", "tenantB");
        
        boolean result = predicate.test(currentRole, baseRole);
        
        assertTrue(result, "Annotated class roles should always be allowed regardless of tenant");
    }

    @Test
    void testCurrentRoleTenantNullBaseRoleTenantNotNull() {
        SameTenantRoleHierarchyCandidatePredicate predicate = new SameTenantRoleHierarchyCandidatePredicate();
        
        // Current role with null tenant, base role with tenant should not match
        BaseRole currentRole = createMockRole(RoleSource.DATABASE, "currentRole", null);
        BaseRole baseRole = createMockRole(RoleSource.DATABASE, "baseRole", "tenantA");
        
        boolean result = predicate.test(currentRole, baseRole);
        
        assertFalse(result, "Role with tenant should not be allowed when current role has null tenant");
    }

    // Mock implementation of BaseRole for testing
    private BaseRole createMockRole(RoleSource source, String code, String tenantId) {
        return new BaseRole() {
            @Override
            public String getCode() {
                return code;
            }

            @Override
            public String getName() {
                return code;
            }

            @Override
            public String getSource() {
                return source != null ? source.name() : null;
            }

            @Override
            public String getTenantId() {
                return tenantId;
            }

            @Override
            public Collection<? extends GrantedAuthority> getAuthorities() {
                return Collections.emptyList();
            }
        };
    }
}
