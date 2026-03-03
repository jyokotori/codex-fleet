-- Add os_type to servers (mac | linux default)
ALTER TABLE servers ADD COLUMN IF NOT EXISTS os_type TEXT NOT NULL DEFAULT 'linux';

-- Add use_docker flag to agents (true = docker, false = no docker)
ALTER TABLE agents ADD COLUMN IF NOT EXISTS use_docker BOOLEAN NOT NULL DEFAULT TRUE;
