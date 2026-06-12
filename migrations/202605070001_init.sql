CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('admin', 'operator', 'viewer')),
  password_hash TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE monitors (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name TEXT NOT NULL,
  url TEXT NOT NULL,
  method TEXT NOT NULL DEFAULT 'GET',
  expected_status INTEGER NOT NULL DEFAULT 200,
  interval_seconds INTEGER NOT NULL DEFAULT 60 CHECK (interval_seconds BETWEEN 15 AND 86400),
  enabled BOOLEAN NOT NULL DEFAULT true,
  last_status TEXT NOT NULL DEFAULT 'pending',
  last_latency_ms INTEGER,
  last_checked_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX monitors_name_url_idx ON monitors (name, url);
CREATE INDEX monitors_due_checks_idx
  ON monitors (enabled, last_checked_at)
  WHERE enabled = true;

CREATE TABLE check_events (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK (status IN ('up', 'down', 'degraded')),
  latency_ms INTEGER,
  status_code INTEGER,
  message TEXT,
  checked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX check_events_monitor_checked_idx ON check_events (monitor_id, checked_at DESC);

CREATE TABLE incidents (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  monitor_id UUID REFERENCES monitors(id) ON DELETE SET NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'resolved')),
  severity TEXT NOT NULL DEFAULT 'minor' CHECK (severity IN ('minor', 'major', 'critical')),
  opened_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  resolved_at TIMESTAMPTZ
);

CREATE INDEX incidents_opened_idx ON incidents (opened_at DESC);
CREATE UNIQUE INDEX incidents_one_open_per_monitor_idx
  ON incidents (monitor_id)
  WHERE status = 'open' AND monitor_id IS NOT NULL;

CREATE TABLE job_runs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  job_type TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
  detail TEXT,
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX job_runs_created_idx ON job_runs (created_at DESC);

CREATE TABLE maintenance_runs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  task_name TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
  output TEXT,
  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ
);

CREATE INDEX maintenance_runs_started_idx ON maintenance_runs (started_at DESC);
