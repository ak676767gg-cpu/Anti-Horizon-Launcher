package com.antihorizon.boot;

import net.fabricmc.loader.api.FabricLoader;

/**
 * Which of the client's builds fits the running game. The jar holds three, each compiled against its own Minecraft:
 * 1.21.11 (com.antihorizon.client), 26.1 and 26.2 (com.antihorizon.client.v26), and 26.3 (com.antihorizon.client.v263).
 */
public final class Gate {
	private static String build;

	private Gate() {
	}

	/** "legacy", "v26" or "v263". */
	public static String build() {
		if (build == null) {
			String v = FabricLoader.getInstance().getModContainer("minecraft").map(c -> c.getMetadata().getVersion().getFriendlyString()).orElse("1");
			build = v.startsWith("1.") ? "legacy" : v.startsWith("26.3") ? "v263" : "v26";
		}
		return build;
	}

	/** The package of the running build's classes. */
	public static String pkg() {
		return build().equals("legacy") ? "com.antihorizon.client" : "com.antihorizon.client." + build();
	}
}
