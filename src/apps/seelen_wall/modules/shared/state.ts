import { computed, signal } from '@preact/signals';
import {
  invoke,
  SeelenCommand,
  SeelenEvent,
  Settings,
  subscribe,
  UIColors,
  WallpaperList,
} from '@seelen-ui/lib';
import { SeelenWallSettings, Wallpaper } from '@seelen-ui/lib/types';

export const $settings = signal<SeelenWallSettings>((await Settings.getAsync()).wall);
Settings.onChange((settings) => ($settings.value = settings.wall));

(await UIColors.getAsync()).setAsCssVariables();
UIColors.onChange((colors) => colors.setAsCssVariables());

export const $paused = signal(false);
subscribe(SeelenEvent.WallStop, ({ payload }) => ($paused.value = payload));

export const $monitors = signal(await invoke(SeelenCommand.SystemGetMonitors));
subscribe(SeelenEvent.SystemMonitorsChanged, ({ payload }) => {
  $monitors.value = payload;
});

export const $wallpapers = signal((await WallpaperList.getAsync()).asArray());
WallpaperList.onChange((wallpapers) => ($wallpapers.value = wallpapers.asArray()));

export const $active_wallpapers = computed(() => {
  const active: Wallpaper[] = [];
  $settings.value.backgroundsV2.forEach((id) => {
    let current = $wallpapers.value.find((w) => w.id === id);
    if (current) {
      active.push(current);
    }
  });
  return active;
});
