-- One row per player who wears something. Nothing else is stored.
CREATE TABLE IF NOT EXISTS looks (
  uuid TEXT PRIMARY KEY,   -- Minecraft player ID, 32 hex digits
  cape TEXT,
  wings TEXT,
  head TEXT,
  updated INTEGER NOT NULL -- when it last changed (ms)
);
