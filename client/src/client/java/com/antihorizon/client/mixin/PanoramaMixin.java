package com.antihorizon.client.mixin;

import com.antihorizon.client.Config;
import com.antihorizon.client.MenuVideo;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.renderer.PanoramaRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Minecraft's own title screen and menus stay as they are; only the spinning panorama behind them becomes the Solas
 * aurora video (MenuVideo).
 */
@Mixin(PanoramaRenderer.class)
public abstract class PanoramaMixin {
	@Inject(method = "render", at = @At("HEAD"), cancellable = true)
	private void antihorizon$aurora(GuiGraphics g, int width, int height, boolean fade, CallbackInfo ci) {
		if (!Config.customMenu) return;
		MenuVideo.draw(g, width, height, 1f);
		ci.cancel();
	}
}
