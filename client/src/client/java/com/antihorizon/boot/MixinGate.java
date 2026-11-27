package com.antihorizon.boot;

import org.objectweb.asm.tree.ClassNode;
import org.spongepowered.asm.mixin.extensibility.IMixinConfigPlugin;
import org.spongepowered.asm.mixin.extensibility.IMixinInfo;

import java.util.List;
import java.util.Set;

/** Applies a mixin config only on the Minecraft versions its build is for (see {@link Gate}). */
public abstract class MixinGate implements IMixinConfigPlugin {
	abstract boolean wanted();

	@Override
	public void onLoad(String mixinPackage) {
	}

	@Override
	public String getRefMapperConfig() {
		return null;
	}

	@Override
	public boolean shouldApplyMixin(String targetClassName, String mixinClassName) {
		return wanted();
	}

	@Override
	public void acceptTargets(Set<String> myTargets, Set<String> otherTargets) {
	}

	@Override
	public List<String> getMixins() {
		return null;
	}

	@Override
	public void preApply(String targetClassName, ClassNode targetClass, String mixinClassName, IMixinInfo mixinInfo) {
	}

	@Override
	public void postApply(String targetClassName, ClassNode targetClass, String mixinClassName, IMixinInfo mixinInfo) {
	}

	/** For the 1.21.11 build's mixins. */
	public static final class Legacy extends MixinGate {
		@Override
		boolean wanted() {
			return Gate.build().equals("legacy");
		}
	}

	/** For the 26.1 and 26.2 build's mixins. */
	public static final class V26 extends MixinGate {
		@Override
		boolean wanted() {
			return Gate.build().equals("v26");
		}
	}

	/** For the 26.3 build's mixins. */
	public static final class V263 extends MixinGate {
		@Override
		boolean wanted() {
			return Gate.build().equals("v263");
		}
	}
}
