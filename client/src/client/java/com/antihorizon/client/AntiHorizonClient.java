package com.antihorizon.client;

import com.mojang.blaze3d.platform.InputConstants;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.rendering.v1.HudRenderCallback;
import net.fabricmc.fabric.api.event.player.AttackEntityCallback;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.world.InteractionResult;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/** The Anti-Horizon client: its menu key works on any screen inside a world or server, and it draws the HUD. */
public final class AntiHorizonClient implements ClientModInitializer {
	public static final Logger LOG = LoggerFactory.getLogger("Anti-Horizon Client");
	private static boolean wasDown;
	private static int watchedKey = -1;

	@Override
	public void onInitializeClient() {
		Config.load();
		ClientTickEvents.END_CLIENT_TICK.register(AntiHorizonClient::watchMenuKey);
		ClientTickEvents.END_CLIENT_TICK.register(Hud::tick);
		HudRenderCallback.EVENT.register((graphics, tick) -> Hud.render(graphics));
		// Combo counter and reach: when you swing at something (the client's own swing, not the server's copy of it).
		AttackEntityCallback.EVENT.register((player, level, hand, entity, hit) -> {
			Minecraft mc = Minecraft.getInstance();
			if (player == mc.player) Hud.onAttack(mc, entity);
			return InteractionResult.PASS;
		});
		if (Config.customMenu) MenuVideo.preload();
		LOG.info("Anti-Horizon Client ready: Right Shift opens the menu in a world or server");
	}

	// Checked every tick, in game and in menus alike, so the menu opens from any screen while playing.
	private static void watchMenuKey(Minecraft mc) {
		boolean down = InputConstants.isKeyDown(mc.getWindow(), Config.menuKey);
		// Just rebound: the new key is still held from setting it, so don't treat that as a press.
		if (watchedKey != Config.menuKey) {
			watchedKey = Config.menuKey;
			wasDown = down;
			return;
		}
		if (down && !wasDown && !ClientMenu.listening) toggle(mc);
		wasDown = down;
	}

	private static void toggle(Minecraft mc) {
		Screen screen = mc.screen;
		// The key may type a letter: chat and text boxes (the menu's own search too) keep it.
		if (screen instanceof ChatScreen || (screen != null && screen.getFocused() instanceof EditBox)) return;
		if (screen instanceof ClientMenu menu && menu.typing()) return;
		// Only in a world or on a server: not on the title screen or the world and server lists.
		if (mc.level == null) return;
		if (screen instanceof ClientMenu menu) {
			if (Config.keybindCloses) menu.onClose();
			else mc.setScreen(new HudEditor(menu));
			return;
		}
		if (screen instanceof HudEditor editor) {
			if (Config.keybindCloses) mc.setScreen(editor.parent() instanceof ClientMenu menu ? menu.parent() : editor.parent());
			else editor.onClose();
			return;
		}
		mc.setScreen(new ClientMenu(screen));
	}
}
