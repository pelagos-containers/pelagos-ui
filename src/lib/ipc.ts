/**
 * Typed wrappers around Tauri's invoke().
 * The frontend calls these instead of invoke() directly.
 */
import { invoke } from '@tauri-apps/api/core';

export interface ContainerInfo {
  name:        string;
  status:      'running' | 'exited';
  pid:         number;
  started_at:  string;   // ISO 8601
  rootfs:      string;
  command:     string[];
  image?:      string;
  exit_code?:  number;
  health?:     'starting' | 'healthy' | 'unhealthy' | 'none';
  bridge_ip?:  string;
  network_ips?: Record<string, string>;
  labels?:     Record<string, string>;
}

export const listContainers  = ()                              => invoke<ContainerInfo[]>('list_containers');
export const stopContainer   = (name: string)                  => invoke<void>('stop_container',   { name });
export const removeContainer = (name: string, force: boolean)  => invoke<void>('remove_container', { name, force });
export const ping            = ()                              => invoke<boolean>('ping');
export const runContainer      = (image: string, name: string | null, args: string[], ports: string[], volumes: string[]) =>
  invoke<number>('run_container', { image, name, args, detach: true, ports, volumes });
export const launchInteractive = (image: string, name: string | null, args: string[], ports: string[], volumes: string[]) =>
  invoke<void>('launch_interactive', { image, name, args, ports, volumes });

export interface ImageInfo {
  reference: string;
  digest:    string;
  layers:    string[];
}

export const listImages   = ()                    => invoke<ImageInfo[]>('list_images');
export const pullImage    = (reference: string)   => invoke<number>('pull_image', { reference });
export const removeImage  = (reference: string)   => invoke<void>('remove_image', { reference });
