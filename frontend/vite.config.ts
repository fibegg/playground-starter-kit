import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "VITE_");
  const backend = env.VITE_API_ORIGIN ?? "http://web:3000";
  const usePolling = env.VITE_USE_POLLING === "true";
  const allowedHostsEnv = env.VITE_ALLOWED_HOSTS?.trim();
  const allowedHosts =
    allowedHostsEnv === "true"
      ? true
      : allowedHostsEnv?.split(",").map((host) => host.trim()).filter(Boolean);

  return {
    plugins: [react(), tailwindcss()],
    server: {
      host: "0.0.0.0",
      port: 5173,
      strictPort: true,
      allowedHosts: Array.isArray(allowedHosts) && allowedHosts.length > 0 ? allowedHosts : true,
      hmr: {
        ...(env.VITE_HMR_HOST ? { host: env.VITE_HMR_HOST } : {}),
        ...(env.VITE_HMR_PROTOCOL ? { protocol: env.VITE_HMR_PROTOCOL as "ws" | "wss" } : {}),
        ...(env.VITE_HMR_CLIENT_PORT ? { clientPort: Number(env.VITE_HMR_CLIENT_PORT) } : {})
      },
      watch: {
        usePolling,
        ignored: ["**/.git/**", "**/target/**", "**/node_modules/.cache/**"]
      },
      proxy: {
        "/graphql": backend,
        "/graphiql": backend,
        "/auth": backend,
        "/api": {
          target: backend,
          ws: true
        },
        "/metrics": backend,
        "/readyz": backend,
        "/up": backend
      }
    },
    build: {
      outDir: "dist",
      emptyOutDir: true,
      sourcemap: true,
      target: "es2022"
    }
  };
});
