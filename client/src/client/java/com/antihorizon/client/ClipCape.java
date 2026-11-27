package com.antihorizon.client;

import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
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
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * A live cape: one of the Minecraft recordings, looping on the cape. Like the title screen's background (MenuVideo),
 * its frames are JPEGs in an AHV1 file that comes from anti-horizon.com the first time, checked against its SHA-256,
 * into ~/.anti-horizon/clips. One frame ahead is decoded on a low-priority thread, and each pair is blended on the GPU
 * (CosmeticsLayer draws the second over the first), so it plays smoothly at any frame rate. One player per clip, however
 * many people wear it; nothing is decoded while nobody wearing it is on screen.
 */
final class ClipCape {
	private static final String BASE = "https://anti-horizon.com/client/clips/";
	private static final Path DIR = Path.of(System.getProperty("user.home"), ".anti-horizon", "clips");
	private static final ExecutorService DECODER = Executors.newSingleThreadExecutor(r -> {
		Thread t = new Thread(r, "Anti-Horizon live capes");
		t.setDaemon(true);
		t.setPriority(Thread.MIN_PRIORITY);
		return t;
	});
	private static final Map<String, ClipCape> PLAYERS = new HashMap<>();

	/** The two frames to show and how far between them (0 = only a, 1 = only b). */
	record Frame(Identifier a, Identifier b, float t) {
	}

	private final String id;
	private final Cosmetics.Clip clip;
	private CompletableFuture<Boolean> loading;
	private byte[] data;
	private int[] offsets;
	private int[] lengths;
	private int count;
	private int fps;
	private int w;
	private int h;
	private final Identifier[] ids = new Identifier[2];
	private final MenuVideo.Linear[] tex = new MenuVideo.Linear[2];
	private final int[] held = {-1, -1};
	private NativeImage staging;
	private CompletableFuture<Void> pending;
	private int pendingFrame = -1;

	private ClipCape(String id, Cosmetics.Clip clip) {
		this.id = id;
		this.clip = clip;
	}

	/** What the live cape `id` shows now, or null while its clip is still on its way (show the still instead). */
	static Frame frame(String id, Cosmetics.Clip clip) {
		return PLAYERS.computeIfAbsent(id, k -> new ClipCape(k, clip)).now();
	}

	private Frame now() {
		if (loading == null) loading = CompletableFuture.supplyAsync(this::read, DECODER);
		if (!loading.isDone() || !loading.join()) return null;
		if (tex[0] == null) {
			for (int k = 0; k < 2; k++) {
				tex[k] = new MenuVideo.Linear("Anti-Horizon live cape " + id + " " + k, w, h);
				ids[k] = Identifier.fromNamespaceAndPath("antihorizon", "clip/" + id + "_" + k);
				Minecraft.getInstance().getTextureManager().register(ids[k], tex[k]);
			}
			staging = new NativeImage(w, h, false);
		}
		double pos = (System.nanoTime() / 1e9 * fps) % count;
		int i = (int) pos;
		int j = (i + 1) % count;
		float t = (float) (pos - i);
		// A finished frame goes into whichever texture holds neither frame being shown.
		if (pending != null && pending.isDone()) {
			int free = held[0] != i && held[0] != j ? 0 : held[1] != i && held[1] != j ? 1 : -1;
			if (free >= 0) {
				tex[free].getPixels().copyFrom(staging);
				tex[free].upload();
				held[free] = pendingFrame;
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
		int a = slot(i), b = slot(j);
		if (a < 0) {
			// Just started, or falling behind: whatever frame is loaded, until the right ones arrive.
			a = held[0] >= 0 ? 0 : held[1] >= 0 ? 1 : -1;
			if (a < 0) return null;
			return new Frame(ids[a], null, 0f);
		}
		return new Frame(ids[a], b >= 0 && b != a ? ids[b] : null, t);
	}

	private int slot(int frame) {
		return held[0] == frame ? 0 : held[1] == frame ? 1 : -1;
	}

	private boolean read() {
		try {
			byte[] file = cached();
			if (file == null) file = download();
			if (file == null) return false;
			ByteBuffer in = ByteBuffer.wrap(file);
			byte[] magic = new byte[4];
			in.get(magic);
			if (!"AHV1".equals(new String(magic, StandardCharsets.US_ASCII))) return false;
			int n = in.getInt();
			fps = in.getInt();
			w = in.getInt();
			h = in.getInt();
			int[] len = new int[n];
			int[] off = new int[n];
			for (int i = 0; i < n; i++) len[i] = in.getInt();
			int at = in.position();
			for (int i = 0; i < n; i++) {
				off[i] = at;
				at += len[i];
			}
			data = file;
			lengths = len;
			offsets = off;
			count = n;
			return n > 1 && fps > 0;
		} catch (Exception e) {
			AntiHorizonClient.LOG.warn("Couldn't read the {} live cape", id, e);
			return false;
		}
	}

	private byte[] cached() {
		try {
			Path p = DIR.resolve(clip.file());
			if (!Files.isRegularFile(p) || Files.size(p) != clip.size()) return null;
			byte[] file = Files.readAllBytes(p);
			return clip.sha256().equals(sha256(file)) ? file : null;
		} catch (Exception e) {
			return null;
		}
	}

	private byte[] download() {
		try {
			HttpURLConnection c = (HttpURLConnection) URI.create(BASE + clip.file()).toURL().openConnection();
			c.setConnectTimeout(10000);
			c.setReadTimeout(30000);
			c.setRequestProperty("User-Agent", "Anti-Horizon-Client");
			byte[] file;
			try (InputStream in = c.getInputStream()) {
				file = in.readAllBytes();
			}
			if (file.length != clip.size() || !clip.sha256().equals(sha256(file))) {
				AntiHorizonClient.LOG.warn("The {} live cape download didn't match; showing its still", id);
				return null;
			}
			Files.createDirectories(DIR);
			Path p = DIR.resolve(clip.file());
			Path part = p.resolveSibling(p.getFileName() + ".part");
			Files.write(part, file);
			Files.move(part, p, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
			return file;
		} catch (Exception e) {
			AntiHorizonClient.LOG.info("The {} live cape isn't downloaded yet ({}); showing its still", id, e.toString());
			return null;
		}
	}

	private static String sha256(byte[] b) throws Exception {
		return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(b));
	}

	/** Decodes frame i into the staging image. Runs on the decoder thread. */
	private void decode(int i) {
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
}
