CREATE INDEX IF NOT EXISTS idx_papers_fts ON papers
    USING fts (title, authors, abstract_text, journal, fulltext)
    WITH (weights = 'title=3.0,authors=2.0,abstract_text=1.5,journal=1.0,fulltext=1.0')
