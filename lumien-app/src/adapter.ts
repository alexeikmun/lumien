import { invoke } from '@tauri-apps/api/core';
import type { Scene } from './scene';
import type { sensors } from './metrics';

export type SensorKey = typeof sensors[number]['key'];
interface Telemetry {
  readings: Record<string, number | null>;
  cpuTempStatus: string | null;
}

export interface DeviceStatus {
  connected: boolean;
  message: string;
}

export interface DeviceAdapter {
  getStatus(): Promise<DeviceStatus>;
  applyScene(scene: Scene): Promise<string>;
  clearDisplay(): Promise<string>;
  getTelemetry(keys: SensorKey[]): Promise<Telemetry>;
}

export class RealDeviceAdapter implements DeviceAdapter {
  private isTauri(): boolean {
    return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
  }

  async getStatus(): Promise<DeviceStatus> {
    if (!this.isTauri()) {
      return { connected: false, message: 'Browser Preview (No device)' };
    }
    try {
      const msg = await invoke<string>('get_device_status');
      return { connected: msg.includes('Connected'), message: msg };
    } catch (e) {
      return { connected: false, message: String(e) };
    }
  }

  async applyScene(scene: Scene): Promise<string> {
    if (!this.isTauri()) {
      return `Preview: validated ${scene.widgets.filter((w) => w.visible).length} widgets. Open in desktop app to write to hardware.`;
    }
    return await invoke<string>('apply_scene', { sceneJson: JSON.stringify(scene) });
  }

  async clearDisplay(): Promise<string> {
    if (!this.isTauri()) {
      return 'Overlay cleared';
    }
    return await invoke<string>('clear_display');
  }

  async getTelemetry(keys: SensorKey[]): Promise<Telemetry> {
    if (!this.isTauri()) {
      return { readings: {}, cpuTempStatus: null };
    }
    return await invoke<Telemetry>('get_telemetry', { keys });
  }
}

export const device: DeviceAdapter = new RealDeviceAdapter();
