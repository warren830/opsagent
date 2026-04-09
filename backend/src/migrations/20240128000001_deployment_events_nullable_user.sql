-- Allow automated deployment events (ArgoCD sync, rollout watcher) without a user
ALTER TABLE deployment_events ALTER COLUMN user_id DROP NOT NULL;
