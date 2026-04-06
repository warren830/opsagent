-- Extend cloud_accounts with role_arn, profile, regions, source for proper credential management
ALTER TABLE cloud_accounts ADD COLUMN IF NOT EXISTS role_arn VARCHAR(500);
ALTER TABLE cloud_accounts ADD COLUMN IF NOT EXISTS profile VARCHAR(100);
ALTER TABLE cloud_accounts ADD COLUMN IF NOT EXISTS regions TEXT[] NOT NULL DEFAULT ARRAY['us-east-1'];
ALTER TABLE cloud_accounts ADD COLUMN IF NOT EXISTS source VARCHAR(20) NOT NULL DEFAULT 'manual';
-- source: 'manual' (user-created) | 'organization' (auto-discovered via AWS Organizations)
