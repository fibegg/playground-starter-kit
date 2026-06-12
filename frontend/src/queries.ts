export type Session = {
  user: null | {
    id: string;
    email: string;
    name: string;
    role: string;
  };
};

export type Dashboard = {
  monitorCount: number;
  upCount: number;
  downCount: number;
  openIncidentCount: number;
  avgLatencyMs: number | null;
};

export type Monitor = {
  id: string;
  name: string;
  url: string;
  expectedStatus: number;
  intervalSeconds: number;
  enabled: boolean;
  lastStatus: string;
  lastLatencyMs: number | null;
  lastCheckedAt: string | null;
};

export type Incident = {
  id: string;
  monitorId: string | null;
  title: string;
  status: string;
  severity: string;
  openedAt: string;
  resolvedAt: string | null;
};

export type JobRun = {
  id: string;
  jobType: string;
  status: string;
  detail: string | null;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
};

export type MaintenanceTask = {
  name: string;
  description: string;
  dangerous: boolean;
};

export async function graphql<T>(query: string, variables?: Record<string, unknown>): Promise<T> {
  const response = await fetch("/graphql", {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  const payload = await response.json();
  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error: { message: string }) => error.message).join(", "));
  }
  return payload.data as T;
}

export async function login(email: string, password: string) {
  const response = await fetch("/auth/login", {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ email, password }),
  });
  if (!response.ok) throw new Error("invalid credentials");
  return response.json() as Promise<Session>;
}

export async function logout() {
  await fetch("/auth/logout", { method: "POST", credentials: "include" });
}
