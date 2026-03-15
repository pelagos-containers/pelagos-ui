import { writable } from 'svelte/store';
import { listContainers, type ContainerInfo } from '../ipc';

export const containers = writable<ContainerInfo[]>([]);
export const loading    = writable(true);
export const error      = writable<string | null>(null);

const POLL_MS = 3000;

export function startPolling(): () => void {
  let timer: ReturnType<typeof setInterval>;

  async function refresh() {
    try {
      containers.set(await listContainers());
      error.set(null);
    } catch (e) {
      error.set(String(e));
    } finally {
      loading.set(false);
    }
  }

  refresh();
  timer = setInterval(refresh, POLL_MS);
  return () => clearInterval(timer);
}
