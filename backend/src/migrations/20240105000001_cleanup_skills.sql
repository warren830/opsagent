-- Clean up legacy skills without repo_path (created by old npx CLI or mock seeds)
DELETE FROM skills WHERE repo_path IS NULL AND git_url IS NULL;
