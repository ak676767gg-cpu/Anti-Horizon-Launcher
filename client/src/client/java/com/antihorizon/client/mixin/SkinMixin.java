package com.antihorizon.client.mixin;

import com.antihorizon.client.Cosmetics;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.world.entity.player.PlayerSkin;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * With an Anti-Horizon cape on, Minecraft draws no cape of its own (CosmeticsLayer draws the moving one) and shows the
 * cape's art on elytra.
 */
@Mixin(AbstractClientPlayer.class)
public abstract class SkinMixin {
	@Inject(method = "getSkin", at = @At("RETURN"), cancellable = true)
	private void antihorizon$cape(CallbackInfoReturnable<PlayerSkin> cir) {
		PlayerSkin skin = cir.getReturnValue();
		PlayerSkin withCape = Cosmetics.skin((AbstractClientPlayer) (Object) this, skin);
		if (withCape != skin) cir.setReturnValue(withCape);
	}
}
