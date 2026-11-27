package com.antihorizon.client;

import com.mojang.blaze3d.platform.InputConstants;
import net.fabricmc.loader.api.FabricLoader;

import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.Set;
import java.util.regex.Pattern;

/** Everything the player can change, kept in config/antihorizon-client.properties. */
public final class Config {
	/** The HUD modules that are switched on, by id. */
	public static final Set<String> enabled = new LinkedHashSet<>(List.of("fps"));

	// Preferences
	public static boolean pauseGame = false;
	public static boolean keybindCloses = true;
	public static boolean rememberPage = true;
	public static boolean animations = true;
	public static int menuKey = InputConstants.KEY_RSHIFT;
	/** The Solas aurora video behind Minecraft's title screen and menus, instead of the panorama. */
	public static boolean customMenu = true;

	/** Where each HUD piece sits, as 0..1 across and down the space it can move in. */
	public static final Map<String, float[]> positions = new LinkedHashMap<>();
	/** Each HUD piece's size, 0.5 to 2 (1 is 100%). */
	public static final Map<String, Float> scales = new HashMap<>();

	static final float MIN_SCALE = 0.5f;
	static final float MAX_SCALE = 2f;

	private static final Path FILE = FabricLoader.getInstance().getConfigDir().resolve("antihorizon-client.properties");

	private Config() {
	}

	static float[] position(String id, float x, float y) {
		return positions.computeIfAbsent(id, k -> new float[] {x, y});
	}

	static float scale(String id) {
		return scales.getOrDefault(id, 1f);
	}

	/** Sizes go in quarter steps, so text stays on whole screen pixels at the usual GUI scales. */
	static void setScale(String id, float s) {
		float v = Math.round(Math.max(MIN_SCALE, Math.min(MAX_SCALE, s)) * 4f) / 4f;
		if (v == 1f) scales.remove(id);
		else scales.put(id, v);
	}

	public static void load() {
		Properties p = new Properties();
		try (Reader r = Files.newBufferedReader(FILE)) {
			p.load(r);
		} catch (IOException e) {
			return; // first run: keep the defaults
		}
		String hud = p.getProperty("hud");
		enabled.clear();
		if (hud != null) {
			for (String id : hud.split(",")) if (!id.isBlank()) enabled.add(id.trim());
		} else {
			// Settings from before the module list: three switches of their own.
			if (bool(p, "fps", true)) enabled.add("fps");
			if (bool(p, "coords", false)) enabled.add("coords");
			if (bool(p, "keystrokes", false)) enabled.add("keys");
		}
		pauseGame = bool(p, "pauseGame", pauseGame);
		keybindCloses = bool(p, "keybindCloses", keybindCloses);
		rememberPage = bool(p, "rememberPage", rememberPage);
		animations = bool(p, "animations", animations);
		menuKey = num(p, "menuKey", menuKey);
		customMenu = bool(p, "customMenu", customMenu);
		for (String key : p.stringPropertyNames()) {
			try {
				if (key.startsWith("pos.")) {
					String[] xy = p.getProperty(key).split(",");
					positions.put(key.substring(4), new float[] {clamp(Float.parseFloat(xy[0])), clamp(Float.parseFloat(xy[1]))});
				} else if (key.startsWith("scale.")) {
					setScale(key.substring(6), Float.parseFloat(p.getProperty(key)));
				}
			} catch (RuntimeException ignored) {
				// a hand-edited value that doesn't parse keeps the default
			}
		}
	}

	public static void save() {
		Properties p = new Properties();
		p.setProperty("hud", String.join(",", enabled));
		p.setProperty("pauseGame", String.valueOf(pauseGame));
		p.setProperty("keybindCloses", String.valueOf(keybindCloses));
		p.setProperty("rememberPage", String.valueOf(rememberPage));
		p.setProperty("animations", String.valueOf(animations));
		p.setProperty("menuKey", String.valueOf(menuKey));
		p.setProperty("customMenu", String.valueOf(customMenu));
		positions.forEach((id, xy) -> p.setProperty("pos." + id, xy[0] + "," + xy[1]));
		scales.forEach((id, s) -> p.setProperty("scale." + id, String.valueOf(s)));
		try {
			Files.createDirectories(FILE.getParent());
			try (Writer w = Files.newBufferedWriter(FILE)) {
				p.store(w, "Anti-Horizon Client");
			}
		} catch (IOException e) {
			AntiHorizonClient.LOG.warn("Couldn't save the client settings", e);
		}
	}

	static float clamp(float v) {
		return Math.max(0f, Math.min(1f, v));
	}

	private static boolean bool(Properties p, String key, boolean fallback) {
		String v = p.getProperty(key);
		return v == null ? fallback : Boolean.parseBoolean(v);
	}

	private static int num(Properties p, String key, int fallback) {
		try {
			return Integer.parseInt(p.getProperty(key, String.valueOf(fallback)).trim());
		} catch (NumberFormatException e) {
			return fallback;
		}
	}
}
