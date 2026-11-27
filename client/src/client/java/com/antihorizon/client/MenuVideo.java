package com.antihorizon.client;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.textures.FilterMode;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;
import org.lwjgl.stb.STBImage;
import org.lwjgl.system.MemoryStack;
import org.lwjgl.system.MemoryUtil;

import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URI;
import java.nio.ByteBuffer;
import java.nio.IntBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * The title screen's moving background: a 20-second loop of the aurora over the flower field at 1920x1080, 240 JPEG
 * frames, 12 a second. One frame ahead is always being decoded on a low-priority thread, and each pair of frames is
 * blended on the GPU, so it plays smoothly at any frame rate. Nothing runs while the title screen isn't showing.
 *
 * At 22 MB it's too big to ship inside the mod (and the launcher), so it comes from anti-horizon.com the first time,
 * checked against its SHA-256, into ~/.anti-horizon where every instance on the PC shares it. Until then, and offline,
 * the screen shows a still frame that is in the mod.
 */
public final class MenuVideo {
	private static final String URL = "https://anti-horizon.com/client/menu-background-1.ahv";
	private static final long SIZE = 23108561L;
	private static final String SHA256 = "bd0ec909c77b2ea751170f3917d96bb0fe9384ada569552707134bef2c9ea0b7";
	private static final Path CACHE = Path.of(System.getProperty("user.home"), ".anti-horizon", "menu-background-1.ahv");
	private static final ExecutorService DECODER = Executors.newSingleThreadExecutor(r -> {
		Thread t = new Thread(r, "Anti-Horizon menu video");
		t.setDaemon(true);
		t.setPriority(Thread.MIN_PRIORITY);
		return t;
	});

	private static CompletableFuture<Boolean> loading;
	private static byte[] data;
	private static int[] offsets;
	private static int[] lengths;
	private static int count;
	private static int fps;
	private static int w;
	private static int h;

	private static final Identifier[] IDS = {id("menu/video_a"), id("menu/video_b")};
	private static final Linear[] TEX = new Linear[2];
	private static final int[] HELD = {-1, -1};
	private static NativeImage staging;
	private static CompletableFuture<Void> pending;
	private static int pendingFrame = -1;
	private static long start;

	private MenuVideo() {
	}

	/** Starts reading the frames from the mod file in the background, so the title screen has them at once. */
	static void preload() {
		load();
	}

	private static synchronized CompletableFuture<Boolean> load() {
		if (loading == null) loading = CompletableFuture.supplyAsync(MenuVideo::read, DECODER);
		return loading;
	}

	private static boolean read() {
		try {
			byte[] file = cached();
			if (file == null) file = download();
			if (file == null) return false;
			ByteBuffer in = ByteBuffer.wrap(file);
			byte[] magic = new byte[4];
			in.get(magic);
			if (!"AHV1".equals(new String(magic, StandardCharsets.US_ASCII))) return false;
			int n = in.getInt();
			int f = in.getInt();
			int ww = in.getInt();
			int hh = in.getInt();
			int[] len = new int[n];
			int[] off = new int[n];
			int at = 20 + 4 * n;
			for (int i = 0; i < n; i++) {
				len[i] = in.getInt();
				off[i] = at;
				at += len[i];
			}
			if (at > file.length) return false;
			data = file;
			offsets = off;
			lengths = len;
			count = n;
			fps = f;
			w = ww;
			h = hh;
			return n > 1;
		} catch (Exception e) {
			AntiHorizonClient.LOG.warn("Couldn't read the menu background", e);
			return false;
		}
	}

	/** The background from the shared folder, if it's there and intact. */
	private static byte[] cached() {
		try {
			if (!Files.isRegularFile(CACHE) || Files.size(CACHE) != SIZE) return null;
			byte[] file = Files.readAllBytes(CACHE);
			return SHA256.equals(sha256(file)) ? file : null;
		} catch (Exception e) {
			return null;
		}
	}

	/** Downloads the background once, checks it, and keeps it for every instance. Null when offline. */
	private static byte[] download() {
		try {
			HttpURLConnection c = (HttpURLConnection) URI.create(URL).toURL().openConnection();
			c.setConnectTimeout(10000);
			c.setReadTimeout(30000);
			c.setRequestProperty("User-Agent", "Anti-Horizon-Client");
			byte[] file;
			try (InputStream in = c.getInputStream()) {
				file = in.readAllBytes();
			}
			if (file.length != SIZE || !SHA256.equals(sha256(file))) {
				AntiHorizonClient.LOG.warn("The menu background download didn't match; keeping the still picture");
				return null;
			}
			Files.createDirectories(CACHE.getParent());
			Path part = CACHE.resolveSibling(CACHE.getFileName() + ".part");
			Files.write(part, file);
			Files.move(part, CACHE, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
			return file;
		} catch (Exception e) {
			AntiHorizonClient.LOG.info("Menu background not downloaded yet ({}); showing the still picture", e.toString());
			return null;
		}
	}

	private static String sha256(byte[] b) throws Exception {
		return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(b));
	}

	/** The still frame in the mod, shown until the moving background is ready. */
	private static Tex still() {
		if (!PICTURES.containsKey("menu_still")) {
			Tex tex = null;
			try (InputStream in = MenuVideo.class.getResourceAsStream("/assets/antihorizon/menu/still.jpg")) {
				NativeImage img = jpeg(in.readAllBytes());
				Identifier id = id("picture/menu_still");
				Minecraft.getInstance().getTextureManager().register(id, new Linear("Anti-Horizon menu still", img));
				tex = new Tex(id, img.getWidth(), img.getHeight());
			} catch (Exception e) {
				// no still: the screen stays dark navy until the video is ready
			}
			PICTURES.put("menu_still", tex);
		}
		return PICTURES.get("menu_still");
	}

	/** A JPEG decoded with STB into a new image (NativeImage itself only reads PNG). */
	private static NativeImage jpeg(byte[] bytes) {
		ByteBuffer src = MemoryUtil.memAlloc(bytes.length);
		try {
			src.put(bytes).flip();
			try (MemoryStack stack = MemoryStack.stackPush()) {
				IntBuffer pw = stack.mallocInt(1);
				IntBuffer ph = stack.mallocInt(1);
				IntBuffer pc = stack.mallocInt(1);
				ByteBuffer px = STBImage.stbi_load_from_memory(src, pw, ph, pc, 4);
				if (px == null) throw new IllegalStateException("not a picture");
				try {
					NativeImage img = new NativeImage(pw.get(0), ph.get(0), false);
					MemoryUtil.memCopy(MemoryUtil.memAddress(px), img.getPointer(), (long) pw.get(0) * ph.get(0) * 4);
					return img;
				} finally {
					STBImage.stbi_image_free(px);
				}
			}
		} finally {
			MemoryUtil.memFree(src);
		}
	}

	/** Decodes frame i into the staging image. Runs on the decoder thread. */
	private static void decode(int i) {
		ByteBuffer src = MemoryUtil.memAlloc(lengths[i]);
		try {
			src.put(data, offsets[i], lengths[i]).flip();
			try (MemoryStack stack = MemoryStack.stackPush()) {
				IntBuffer pw = stack.mallocInt(1);
				IntBuffer ph = stack.mallocInt(1);
				IntBuffer pc = stack.mallocInt(1);
				ByteBuffer px = STBImage.stbi_load_from_memory(src, pw, ph, pc, 4);
				if (px == null) return;
				try {
					if (pw.get(0) == w && ph.get(0) == h) MemoryUtil.memCopy(MemoryUtil.memAddress(px), staging.getPointer(), (long) w * h * 4);
				} finally {
					STBImage.stbi_image_free(px);
				}
			}
		} finally {
			MemoryUtil.memFree(src);
		}
	}

	/** Draws the background to cover the whole screen, at the given opacity (for fading in). */
	public static void draw(GuiGraphics g, int width, int height, float alpha) {
		g.fill(0, 0, width, height, 0xFF070E15);
		CompletableFuture<Boolean> l = load();
		boolean ready = l.isDone() && l.join();
		if (!ready || (HELD[0] < 0 && HELD[1] < 0)) {
			// Not ready (first run, still downloading, or offline): the still frame, scaled to cover.
			Tex still = still();
			if (still != null) {
				float sc = Math.max((float) width / still.width(), (float) height / still.height());
				int sw = (int) Math.ceil(still.width() * sc);
				int sh = (int) Math.ceil(still.height() * sc);
				int a = Math.round(Math.max(0f, Math.min(1f, alpha)) * 255f);
				g.blit(RenderPipelines.GUI_TEXTURED, still.id(), (width - sw) / 2, (height - sh) / 2, 0, 0, sw, sh, still.width(), still.height(),
						still.width(), still.height(), (a << 24) | 0xFFFFFF);
			}
			if (!ready) return;
		}
		if (TEX[0] == null) {
			for (int k = 0; k < 2; k++) {
				TEX[k] = new Linear("Anti-Horizon menu video " + k, w, h);
				Minecraft.getInstance().getTextureManager().register(IDS[k], TEX[k]);
			}
			staging = new NativeImage(w, h, false);
		}
		long now = System.nanoTime();
		if (start == 0) start = now;
		double pos = ((now - start) / 1e9 * fps) % count;
		int i = (int) pos;
		int j = (i + 1) % count;
		float t = (float) (pos - i);

		// A finished frame goes into whichever texture holds neither frame being shown.
		if (pending != null && pending.isDone()) {
			int free = HELD[0] != i && HELD[0] != j ? 0 : HELD[1] != i && HELD[1] != j ? 1 : -1;
			if (free >= 0) {
				TEX[free].getPixels().copyFrom(staging);
				TEX[free].upload();
				HELD[free] = pendingFrame;
				pending = null;
			}
		}
		if (pending == null) {
			int want = slot(i) < 0 ? i : slot(j) < 0 ? j : (j + 1) % count;
			if (slot(want) < 0) {
				pendingFrame = want;
				pending = CompletableFuture.runAsync(() -> decode(want), DECODER);
			}
		}

		int a = slot(i);
		int b = slot(j);
		if (a < 0 && b < 0) {
			// Just started: show whatever is loaded until the right frames arrive.
			a = HELD[0] >= 0 ? 0 : HELD[1] >= 0 ? 1 : -1;
			if (a < 0) return;
		}
		float scale = Math.max((float) width / w, (float) height / h);
		int dw = (int) Math.ceil(w * scale);
		int dh = (int) Math.ceil(h * scale);
		int x = (width - dw) / 2;
		int y = (height - dh) / 2;
		if (a >= 0) blit(g, a, x, y, dw, dh, alpha);
		if (b >= 0 && b != a && t > 0.01f) blit(g, b, x, y, dw, dh, a >= 0 ? alpha * t : alpha);
	}

	private static int slot(int frame) {
		return HELD[0] == frame ? 0 : HELD[1] == frame ? 1 : -1;
	}

	private static void blit(GuiGraphics g, int k, int x, int y, int dw, int dh, float alpha) {
		int a = Math.round(Math.max(0f, Math.min(1f, alpha)) * 255f);
		g.blit(RenderPipelines.GUI_TEXTURED, IDS[k], x, y, 0, 0, dw, dh, w, h, w, h, (a << 24) | 0xFFFFFF);
	}

	// ---------------------------------------------------------------- small pictures (world, server and mod icons)

	/** A picture and its size in pixels. */
	record Tex(Identifier id, int width, int height) {
	}

	private static final Map<String, Tex> PICTURES = new HashMap<>();

	/** A PNG made into a smoothly scaled texture, once per key. Null if it can't be read. */
	static Tex picture(String key, byte[] png) {
		String k = key.toLowerCase(Locale.ROOT).replaceAll("[^a-z0-9_.-]", "_");
		if (PICTURES.containsKey(k)) return PICTURES.get(k);
		Tex tex = null;
		try {
			NativeImage img = NativeImage.read(png);
			Linear t = new Linear("Anti-Horizon " + k, img);
			Identifier id = id("picture/" + k);
			Minecraft.getInstance().getTextureManager().register(id, t);
			tex = new Tex(id, img.getWidth(), img.getHeight());
		} catch (Exception e) {
			// not a picture we can show: the card falls back to a letter
		}
		PICTURES.put(k, tex);
		return tex;
	}

	/** The Anti-Horizon logo (the black hole with the gold ring), from the mod's own files. */
	static Tex logo() {
		if (!PICTURES.containsKey("logo")) {
			try (InputStream in = MenuVideo.class.getResourceAsStream("/assets/antihorizon/logo.png")) {
				return picture("logo", in.readAllBytes());
			} catch (Exception e) {
				PICTURES.put("logo", null);
			}
		}
		return PICTURES.get("logo");
	}

	static void drawPicture(GuiGraphics g, Tex tex, int x, int y, int size, float alpha) {
		int a = Math.round(Math.max(0f, Math.min(1f, alpha)) * 255f);
		g.blit(RenderPipelines.GUI_TEXTURED, tex.id(), x, y, 0, 0, size, size, tex.width(), tex.height(), tex.width(), tex.height(), (a << 24) | 0xFFFFFF);
	}

	private static Identifier id(String path) {
		return Identifier.fromNamespaceAndPath("antihorizon", path);
	}

	/** A texture that blends between pixels when scaled, so pictures and the video stay smooth at any size. */
	private static final class Linear extends DynamicTexture {
		Linear(String name, int width, int height) {
			super(() -> name, width, height, true);
			this.sampler = RenderSystem.getSamplerCache().getClampToEdge(FilterMode.LINEAR);
		}

		Linear(String name, NativeImage image) {
			super(() -> name, image);
			this.sampler = RenderSystem.getSamplerCache().getClampToEdge(FilterMode.LINEAR);
		}
	}
}
