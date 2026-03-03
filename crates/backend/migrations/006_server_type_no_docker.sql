-- Add os_type to servers (mac | linux default)
ALTER TABLE servers ADD COLUMN os_type TEXT NOT NULL DEFAULT 'linux';

-- Add use_docker flag to agents (1 = docker, 0 = no docker)
ALTER TABLE agents ADD COLUMN use_docker INTEGER NOT NULL DEFAULT 1;
