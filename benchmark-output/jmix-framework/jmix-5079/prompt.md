# jmix-framework/jmix-5079

Fix two bugs in the role management functionality:

1. Fix the NullPointerException that occurs when assigning a role to users. The system should handle role assignment gracefully without throwing NPE.

2. Enable the ability to add base roles during the role creation process. Users should be able to select and assign base roles when creating a new role.

Both issues affect the security role management workflow and need to be resolved to ensure proper role administration.
