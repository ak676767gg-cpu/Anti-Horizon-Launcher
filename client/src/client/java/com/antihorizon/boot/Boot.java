package com.antihorizon.boot;

import net.fabricmc.api.ClientModInitializer;

/**
 * The client's entrypoint on every Minecraft version. One jar holds a build for 1.21.11, one for 26.1 and 26.2, and one
 * for 26.3; this starts whichever matches the game that's running. The others are never loaded.
 */
public final class Boot implements ClientModInitializer {
	@Override
	public void onInitializeClient() {
		String impl = Gate.pkg() + ".AntiHorizonClient";
		try {
			((ClientModInitializer) Class.forName(impl).getDeclaredConstructor().newInstance()).onInitializeClient();
		} catch (ReflectiveOperationException e) {
			throw new IllegalStateException("Anti-Horizon Client has no build for this Minecraft version", e);
		}
	}
}
