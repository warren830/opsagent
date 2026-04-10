-- Simplify roles: tenant_admin → member
UPDATE users SET role = 'member' WHERE role = 'tenant_admin';
