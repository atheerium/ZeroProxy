import { defineConfig } from 'astro/config';
import react from '@astrojs/react';
import tailwind from '@astrojs/tailwind';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

/**
 * Identity of THIS dashboard bundle, frozen at `pnpm build` time.
 *
 * The backend reports its own build identity, but that says nothing about the
 * dashboard: in `--web-dir` mode assets are read from disk at request time, so a
 * freshly compiled binary happily serves a `web/dist/` from days ago. The navbar
 * badge has to name the UI layer separately, and the only value that cannot
 * drift from the code the browser executes is one substituted at compile time.
 *
 * Any git failure yields `undefined` — a missing checkout must not break a build.
 */
function git(...args) {
  try {
    return execFileSync('git', args, {
      cwd: path.resolve(import.meta.dirname, '..'),
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    return undefined;
  }
}

const uiBuiltAt = new Date().toISOString();
const uiGitSha = git('rev-parse', '--short', 'HEAD');
const uiCommitTime = git('show', '-s', '--format=%cI', 'HEAD');

export default defineConfig({
  integrations: [
    react({
      // Optimize React integration
      experimentalReact: true,
    }),
    tailwind({
      // Optimize Tailwind CSS
      applyBaseStyles: false,
    }),
  ],
  // Disable HMR to avoid WebSocket issues during development
  devToolbar: {
    enabled: false,
  },
  output: 'static',
  site: 'https://github.com/yourusername/openproxy-rust',
  base: '/',
  compressHTML: true,
  build: {
    format: 'file', // Better for simple routing
    inlineStylesheets: 'auto', // Better for caching
  },
  vite: {
    // Compile-time constants for the navbar build-freshness badge. See the
    // comment above `uiBuiltAt` for why these live in the bundle rather than
    // being read from the API.
    define: {
      __UI_BUILT_AT__: JSON.stringify(uiBuiltAt),
      __UI_GIT_SHA__: JSON.stringify(uiGitSha ?? 'unknown'),
      __UI_COMMIT_TIME__: JSON.stringify(uiCommitTime ?? 'unknown'),
    },
    server: {
      hmr: false,
      // Dev-only: forward backend API + asset routes to the Rust server on :4623
      // so the dashboard works when running `astro dev` on :4624 against a
      // separate `cargo run -- --port 4623` process.
      proxy: {
        '/api': { target: 'http://127.0.0.1:4623', changeOrigin: true },
        '/v1': { target: 'http://127.0.0.1:4623', changeOrigin: true },
        '/health': { target: 'http://127.0.0.1:4623', changeOrigin: true },
        '/oauth': { target: 'http://127.0.0.1:4623', changeOrigin: true },
      },
    },
    resolve: {
      alias: {
        '@': path.resolve('./src'),
      },
    },
    optimizeDeps: {
      include: ['react', 'react-dom', 'react-is'],
    },
    build: {
      // Optimize bundle size
      rollupOptions: {
        output: {
          manualChunks: {
            // Split React libraries
            'react-vendor': ['react', 'react-dom', 'react-is'],
            // Split UI libraries
            'ui-vendor': ['recharts', '@xyflow/react', '@monaco-editor/react'],
            // Split utility libraries
            'utils-vendor': ['zustand', 'lowdb', 'marked'],
          },
        },
      },
      // Enable minification
      minify: 'terser',
      terserOptions: {
        compress: {
          drop_console: true,
          drop_debugger: true,
          pure_funcs: ['console.log', 'console.info'],
        },
        mangle: {
          safari10: true,
        },
      },
    },
  },
});
