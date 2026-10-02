CREATE INDEX IF NOT EXISTS idx_concepts_fts ON concepts
    USING fts (title, body)
    WITH (weights = 'title=3.0,body=1.0');

CREATE INDEX IF NOT EXISTS idx_claims_fts ON claims
    USING fts (statement, quote)
    WITH (weights = 'statement=3.0,quote=1.0');
