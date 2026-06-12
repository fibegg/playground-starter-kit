import { QueryClient, QueryClientProvider, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Activity, AlertTriangle, CheckCircle2, Database, Lock, Play, Shield, Trash2, Wrench } from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import {
  type Dashboard,
  graphql,
  type Incident,
  type JobRun,
  login,
  logout,
  type MaintenanceTask,
  type Monitor,
  type Session,
} from "./queries";

const queryClient = new QueryClient();
const liveQueryKeys = ["dashboard", "monitors", "incidents", "jobs"] as const;

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <Shell />
    </QueryClientProvider>
  );
}

function Shell() {
  const session = useQuery({ queryKey: ["session"], queryFn: () => fetchSession() });
  if (session.isLoading) return <main className="loading">Loading...</main>;
  if (!session.data?.user) return <Login />;
  return <Console session={session.data as AuthenticatedSession} />;
}

function Login() {
  const qc = useQueryClient();
  const [email, setEmail] = useState("admin@example.com");
  const [password, setPassword] = useState("password");
  const mutation = useMutation({
    mutationFn: () => login(email, password),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["session"] }),
  });
  return (
    <main className="login">
      <section className="login-panel">
        <Shield size={28} />
        <h1>Uptime Console</h1>
        <p>Production-like Rust starter with GraphQL, jobs, Redis, RBAC, and security controls.</p>
        <label>
          Email
          <input value={email} onChange={(event) => setEmail(event.target.value)} />
        </label>
        <label>
          Password
          <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} />
        </label>
        <button type="button" onClick={() => mutation.mutate()} disabled={mutation.isPending}>
          <Lock size={16} /> Sign in
        </button>
        {mutation.isError ? <span className="error">Invalid credentials</span> : null}
      </section>
    </main>
  );
}

type AuthenticatedSession = Session & { user: NonNullable<Session["user"]> };

function Console({ session }: { session: AuthenticatedSession }) {
  const qc = useQueryClient();
  useLiveUpdates();

  const dashboard = useQuery({ queryKey: ["dashboard"], queryFn: fetchDashboard });
  const monitors = useQuery({ queryKey: ["monitors"], queryFn: fetchMonitors });
  const incidents = useQuery({ queryKey: ["incidents"], queryFn: fetchIncidents });
  const jobs = useQuery({ queryKey: ["jobs"], queryFn: fetchJobs, refetchInterval: 2000 });
  const tasks = useQuery({ queryKey: ["tasks"], queryFn: fetchMaintenanceTasks });
  const [form, setForm] = useState({ name: "Docs", url: "https://docs.rs", expectedStatus: 200, intervalSeconds: 60 });
  const openIncidentMonitorIds = new Set(
    (incidents.data ?? [])
      .filter((incident) => incident.status === "open" && incident.monitorId)
      .map((incident) => incident.monitorId as string),
  );

  const createMonitor = useMutation({
    mutationFn: () => createMonitorMutation(form),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["monitors"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
    },
  });
  const enqueue = useMutation({
    mutationFn: (id: string) => enqueueCheckMutation(id),
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: ["monitors"] });
      const previous = qc.getQueryData<Monitor[]>(["monitors"]);
      qc.setQueryData<Monitor[]>(["monitors"], (current) =>
        current?.map((monitor) => (monitor.id === id ? { ...monitor, lastStatus: "pending" } : monitor)),
      );
      return { previous };
    },
    onError: (_error, _id, context) => {
      if (context?.previous) qc.setQueryData(["monitors"], context.previous);
    },
    onSettled: () => invalidateLiveQueries(qc),
  });
  const deleteMonitor = useMutation({
    mutationFn: (id: string) => deleteMonitorMutation(id),
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: ["monitors"] });
      const previous = qc.getQueryData<Monitor[]>(["monitors"]);
      qc.setQueryData<Monitor[]>(["monitors"], (current) => current?.filter((monitor) => monitor.id !== id));
      return { previous };
    },
    onError: (_error, _id, context) => {
      if (context?.previous) qc.setQueryData(["monitors"], context.previous);
    },
    onSettled: () => invalidateLiveQueries(qc),
  });
  const openIncident = useMutation({
    mutationFn: (monitor: Monitor) =>
      openIncidentMutation({
        monitorId: monitor.id,
        title: `${monitor.name} is down`,
        severity: "major",
      }),
    onSettled: () => invalidateLiveQueries(qc),
  });
  const resolveIncident = useMutation({
    mutationFn: (id: string) => resolveIncidentMutation(id),
    onSettled: () => invalidateLiveQueries(qc),
  });
  const runTask = useMutation({
    mutationFn: (name: string) => runMaintenanceTaskMutation(name),
    onSettled: () => invalidateLiveQueries(qc),
  });

  return (
    <main className="app">
      <header className="topbar">
        <div>
          <h1>Uptime Console</h1>
          <span>
            {session.user.name} / {session.user.role}
          </span>
        </div>
        <button
          type="button"
          className="ghost"
          onClick={async () => {
            await logout();
            await qc.invalidateQueries({ queryKey: ["session"] });
          }}
        >
          Sign out
        </button>
      </header>

      <section className="metrics">
        <Metric icon={<Activity />} label="Monitors" value={dashboard.data?.monitorCount ?? 0} />
        <Metric icon={<CheckCircle2 />} label="Up" value={dashboard.data?.upCount ?? 0} tone="ok" />
        <Metric icon={<AlertTriangle />} label="Down" value={dashboard.data?.downCount ?? 0} tone="bad" />
        <Metric icon={<Database />} label="Open incidents" value={dashboard.data?.openIncidentCount ?? 0} />
      </section>

      <section className="grid">
        <Panel title="Monitors">
          <form
            className="inline-form"
            onSubmit={(event) => {
              event.preventDefault();
              createMonitor.mutate();
            }}
          >
            <input value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} />
            <input value={form.url} onChange={(event) => setForm({ ...form, url: event.target.value })} />
            <button type="submit">Add</button>
          </form>
          {(monitors.data ?? []).map((monitor) => (
            <div className="row monitor-row" key={monitor.id}>
              <div>
                <strong>{monitor.name}</strong>
                <span>{monitor.url}</span>
              </div>
              <Status value={monitor.lastStatus} />
              <div className="actions">
                <button
                  type="button"
                  className="icon"
                  onClick={() => openIncident.mutate(monitor)}
                  disabled={monitor.lastStatus !== "down" || openIncidentMonitorIds.has(monitor.id)}
                  title={openIncidentMonitorIds.has(monitor.id) ? "Incident already open" : "Open incident"}
                >
                  <AlertTriangle size={16} />
                </button>
                <button type="button" className="icon" onClick={() => enqueue.mutate(monitor.id)} title="Run check now">
                  <Play size={16} />
                </button>
                <button
                  type="button"
                  className="icon danger"
                  onClick={() => {
                    if (window.confirm(`Delete monitor "${monitor.name}"?`)) deleteMonitor.mutate(monitor.id);
                  }}
                  title="Delete monitor"
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </div>
          ))}
        </Panel>

        <Panel title="Incidents">
          {(incidents.data ?? []).length === 0 ? <p className="empty">No incidents yet.</p> : null}
          {(incidents.data ?? []).map((incident) => (
            <div className="row incident-row" key={incident.id}>
              <div>
                <strong>{incident.title}</strong>
                <span>{incident.severity}</span>
              </div>
              <Status value={incident.status} />
              <button
                type="button"
                className="icon"
                onClick={() => resolveIncident.mutate(incident.id)}
                disabled={incident.status !== "open"}
                title="Resolve incident"
              >
                <CheckCircle2 size={16} />
              </button>
            </div>
          ))}
        </Panel>

        <Panel title="Maintenance">
          {(tasks.data ?? []).map((task) => (
            <div className="row" key={task.name}>
              <div>
                <strong>{task.name}</strong>
                <span>{task.description}</span>
              </div>
              <button type="button" className="icon" onClick={() => runTask.mutate(task.name)} title="Run task">
                <Wrench size={16} />
              </button>
            </div>
          ))}
        </Panel>

        <Panel title="Recent Jobs">
          {(jobs.data ?? []).map((job) => (
            <div className="row compact" key={job.id}>
              <div>
                <strong>{job.jobType}</strong>
                <span>{job.createdAt}</span>
              </div>
              <Status value={job.status} />
            </div>
          ))}
        </Panel>
      </section>
    </main>
  );
}

function useLiveUpdates() {
  const qc = useQueryClient();

  useEffect(() => {
    const source = new EventSource("/api/events", { withCredentials: true });
    const refresh = () => invalidateLiveQueries(qc);
    source.addEventListener("message", refresh);

    return () => {
      source.removeEventListener("message", refresh);
      source.close();
    };
  }, [qc]);
}

function invalidateLiveQueries(client: QueryClient) {
  for (const key of liveQueryKeys) {
    void client.invalidateQueries({ queryKey: [key] });
  }
}

function Metric({ icon, label, value, tone }: { icon: ReactNode; label: string; value: number; tone?: "ok" | "bad" }) {
  return (
    <div className={`metric ${tone ?? ""}`}>
      {icon}
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="panel">
      <h2>{title}</h2>
      {children}
    </section>
  );
}

function Status({ value }: { value: string }) {
  return <span className={`status ${value}`}>{value}</span>;
}

async function fetchSession() {
  const response = await fetch("/auth/session", { credentials: "include" });
  return response.json() as Promise<Session>;
}

async function fetchDashboard() {
  const data = await graphql<{ dashboard: Dashboard }>(
    `query { dashboard { monitorCount upCount downCount openIncidentCount avgLatencyMs } }`,
  );
  return data.dashboard;
}

async function fetchMonitors() {
  const data = await graphql<{ monitors: Monitor[] }>(
    `query { monitors { id name url expectedStatus intervalSeconds enabled lastStatus lastLatencyMs lastCheckedAt } }`,
  );
  return data.monitors;
}

async function fetchIncidents() {
  const data = await graphql<{ incidents: Incident[] }>(
    `query { incidents { id monitorId title status severity openedAt resolvedAt } }`,
  );
  return data.incidents;
}

async function fetchJobs() {
  const data = await graphql<{ jobRuns: JobRun[] }>(
    `query { jobRuns { id jobType status detail createdAt startedAt finishedAt } }`,
  );
  return data.jobRuns;
}

async function fetchMaintenanceTasks() {
  const data = await graphql<{ maintenanceTasks: MaintenanceTask[] }>(
    `query { maintenanceTasks { name description dangerous } }`,
  );
  return data.maintenanceTasks;
}

async function createMonitorMutation(input: {
  name: string;
  url: string;
  expectedStatus: number;
  intervalSeconds: number;
}) {
  const data = await graphql<{ createMonitor: Monitor }>(
    `mutation CreateMonitor($input: MonitorInput!) { createMonitor(input: $input) { id name url lastStatus } }`,
    { input },
  );
  return data.createMonitor;
}

async function enqueueCheckMutation(monitorId: string) {
  return graphql<{ enqueueCheck: string }>(
    `mutation Enqueue($monitorId: UUID!) { enqueueCheck(monitorId: $monitorId) }`,
    { monitorId },
  );
}

async function deleteMonitorMutation(id: string) {
  return graphql<{ deleteMonitor: boolean }>(`mutation DeleteMonitor($id: UUID!) { deleteMonitor(id: $id) }`, { id });
}

async function openIncidentMutation(input: { monitorId: string; title: string; severity: string }) {
  return graphql<{ openIncident: Incident }>(
    `mutation OpenIncident($input: IncidentInput!) { openIncident(input: $input) { id monitorId title status severity openedAt resolvedAt } }`,
    { input },
  );
}

async function resolveIncidentMutation(id: string) {
  return graphql<{ resolveIncident: Incident }>(
    `mutation ResolveIncident($id: UUID!) { resolveIncident(id: $id) { id monitorId title status severity openedAt resolvedAt } }`,
    { id },
  );
}

async function runMaintenanceTaskMutation(name: string) {
  return graphql<{ runMaintenanceTask: { id: string } }>(
    `mutation Run($name: String!) { runMaintenanceTask(name: $name) { id } }`,
    { name },
  );
}

const root = document.getElementById("root");
if (!root) {
  throw new Error("Application root element is missing.");
}

createRoot(root).render(<App />);
