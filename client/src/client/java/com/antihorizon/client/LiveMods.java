package com.antihorizon.client;

import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.ModContainer;
import net.fabricmc.loader.api.entrypoint.EntrypointContainer;
import net.minecraft.client.gui.screens.Screen;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import java.util.function.Function;

/**
 * The installed mods the menu can change while you play: a few that switch on and off at once (Iris shaders, 3D Skin
 * Layers, Dynamic FPS, Entity Culling), and every mod with a settings screen (through Mod Menu's hook). All found by
 * reflection, so none of these mods is needed to build or run the client; a mod that isn't there just isn't listed.
 */
final class LiveMods {
	/** One mod's card: a switch (on and set) and/or its settings screen (config). */
	record Live(String modId, String name, String about, BooleanSupplier on, Consumer<Boolean> set, Function<Screen, Screen> config) {
	}

	private record Toggle(BooleanSupplier on, Consumer<Boolean> set) {
	}

	/** Builds a mod's switch by reflection; may fail if the mod changed its insides. */
	private interface Maker {
		Toggle make() throws Exception;
	}

	private LiveMods() {
	}

	static List<Live> find() {
		Map<String, Function<Screen, Screen>> configs = settingsScreens();
		List<Live> out = new ArrayList<>();
		addToggle(out, configs, "iris", "Shaders on or off, applied straight away.", LiveMods::iris);
		addToggle(out, configs, "skinlayers3d", "Skins in 3D: hat, jacket, sleeves and trousers stand out.", LiveMods::skinLayers);
		addToggle(out, configs, "dynamic_fps", "Saves power by slowing the game down while it's in the background.", LiveMods::dynamicFps);
		addToggle(out, configs, "entityculling", "Skips drawing mobs and blocks you can't see, for more FPS.", LiveMods::entityCulling);
		for (Map.Entry<String, Function<Screen, Screen>> e : configs.entrySet()) {
			String id = e.getKey();
			if (id.equals("antihorizon") || out.stream().anyMatch(l -> l.modId().equals(id))) continue;
			out.add(new Live(id, name(id), "Open its settings.", null, null, e.getValue()));
		}
		out.sort(Comparator.comparing((Live l) -> l.on() == null).thenComparing(l -> l.name().toLowerCase(Locale.ROOT)));
		return out;
	}

	private static void addToggle(List<Live> out, Map<String, Function<Screen, Screen>> configs, String id, String about, Maker make) {
		if (!FabricLoader.getInstance().isModLoaded(id)) return;
		Toggle t;
		try {
			t = make.make();
		} catch (Throwable e) {
			t = null;
		}
		if (t == null && !configs.containsKey(id)) return;
		out.add(new Live(id, name(id), about, t == null ? null : t.on(), t == null ? null : t.set(), configs.get(id)));
	}

	private static String name(String id) {
		String n = FabricLoader.getInstance().getModContainer(id).map(ModContainer::getMetadata).map(m -> m.getName()).orElse(id);
		return switch (id) {
			case "skinlayers3d" -> "3D Skin Layers";
			case "entityculling" -> "Entity Culling";
			case "waveycapes" -> "Wavey Capes";
			default -> n;
		};
	}

	// ---------------------------------------------------------------- switches

	private static Toggle iris() throws ReflectiveOperationException {
		Class<?> api = Class.forName("net.irisshaders.iris.api.v0.IrisApi");
		Object config = api.getMethod("getConfig").invoke(api.getMethod("getInstance").invoke(null));
		Class<?> type = Class.forName("net.irisshaders.iris.api.v0.IrisApiConfig");
		Method get = type.getMethod("areShadersEnabled");
		Method set = type.getMethod("setShadersEnabledAndApply", boolean.class);
		return new Toggle(() -> (Boolean) call(get, config), v -> call(set, config, v));
	}

	private static final String[] LAYERS = {"enableHat", "enableJacket", "enableLeftSleeve", "enableRightSleeve", "enableLeftPants", "enableRightPants"};

	private static Toggle skinLayers() throws ReflectiveOperationException {
		Class<?> base = Class.forName("dev.tr7zw.skinlayers.versionless.ModBase");
		Field configField = base.getField("config");
		Method write = base.getMethod("writeConfig");
		Field instance = Class.forName("dev.tr7zw.skinlayers.SkinLayersModBase").getField("instance");
		return new Toggle(() -> {
			Object c = get(configField, null);
			for (String f : LAYERS) if (Boolean.TRUE.equals(get(field(c, f), c))) return true;
			return false;
		}, v -> {
			Object c = get(configField, null);
			for (String f : LAYERS) put(field(c, f), c, v);
			call(write, get(instance, null));
		});
	}

	private static Toggle dynamicFps() throws ReflectiveOperationException {
		Class<?> mod = Class.forName("dynamic_fps.impl.DynamicFPSMod");
		Method off = mod.getMethod("disabledByUser");
		Method flip = mod.getMethod("toggleDisabled");
		return new Toggle(() -> !(Boolean) call(off, null), v -> {
			boolean disabled = (Boolean) call(off, null);
			if (v == disabled) call(flip, null);
		});
	}

	private static Toggle entityCulling() throws ReflectiveOperationException {
		Field enabled = Class.forName("dev.tr7zw.entityculling.versionless.EntityCullingVersionlessBase").getField("enabled");
		return new Toggle(() -> (Boolean) get(enabled, null), v -> put(enabled, null, v));
	}

	// ---------------------------------------------------------------- settings screens

	/** Each mod's settings screen, from the "modmenu" entrypoint mods use to register one with Mod Menu. */
	private static Map<String, Function<Screen, Screen>> settingsScreens() {
		Map<String, Function<Screen, Screen>> out = new HashMap<>();
		List<EntrypointContainer<Object>> list;
		try {
			list = FabricLoader.getInstance().getEntrypointContainers("modmenu", Object.class);
		} catch (Throwable e) {
			return out;
		}
		for (EntrypointContainer<Object> c : list) {
			try {
				Object api = c.getEntrypoint();
				Method factory = api.getClass().getMethod("getModConfigScreenFactory");
				// Mod Menu's own default returns nothing; only a mod that overrides it has a settings screen.
				if (factory.getDeclaringClass().isInterface()) continue;
				out.put(c.getProvider().getMetadata().getId(), parent -> {
					Object f = call(factory, api);
					if (f == null) return null;
					for (Class<?> i : f.getClass().getInterfaces()) {
						for (Method m : i.getMethods()) {
							if (m.getName().equals("create") && m.getParameterCount() == 1) return (Screen) call(m, f, parent);
						}
					}
					return null;
				});
			} catch (Throwable ignored) {
				// a mod whose Mod Menu hook can't load (Mod Menu not installed): no settings card for it
			}
		}
		return out;
	}

	// ---------------------------------------------------------------- reflection helpers that don't throw

	private static Object call(Method m, Object target, Object... args) {
		try {
			return m.invoke(target, args);
		} catch (ReflectiveOperationException e) {
			throw new IllegalStateException(e);
		}
	}

	private static Field field(Object o, String name) {
		try {
			return o.getClass().getField(name);
		} catch (NoSuchFieldException e) {
			throw new IllegalStateException(e);
		}
	}

	private static Object get(Field f, Object target) {
		try {
			return f.get(target);
		} catch (IllegalAccessException e) {
			throw new IllegalStateException(e);
		}
	}

	private static void put(Field f, Object target, Object value) {
		try {
			f.set(target, value);
		} catch (IllegalAccessException e) {
			throw new IllegalStateException(e);
		}
	}
}
