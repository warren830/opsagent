-- Glossary: term must be unique per account (NULL account treated as distinct)
CREATE UNIQUE INDEX IF NOT EXISTS idx_glossary_term_account ON glossary (term, account_id) WHERE account_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_glossary_term_global ON glossary (term) WHERE account_id IS NULL;
