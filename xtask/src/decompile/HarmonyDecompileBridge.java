// Read-only headless decompilation bridge for EU4 Harmony (`cargo xtask decompile`).
// @category Harmony

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileOptions;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.decompiler.DecompiledFunction;
import ghidra.app.util.headless.HeadlessScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.util.task.TaskMonitorAdapter;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;

public final class HarmonyDecompileBridge extends HeadlessScript {
    private static final Gson GSON = new Gson();
    private static final long DEFAULT_IDLE_TIMEOUT_MS = 15L * 60L * 1000L;
    private static final int DEFAULT_TIMEOUT_SECONDS = 60;

    private DecompInterface decompiler;

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length != 1) {
            throw new IllegalArgumentException("HarmonyDecompileBridge expects one config file path argument");
        }
        Path configPath = Path.of(args[0]);
        JsonObject config = JsonParser.parseString(Files.readString(configPath, StandardCharsets.UTF_8)).getAsJsonObject();
        String mode = requireString(config, "mode");

        initDecompiler();
        try {
            if ("oneshot".equals(mode)) {
                runOneshot(config);
            } else if ("daemon".equals(mode)) {
                runDaemon(config);
            } else {
                throw new IllegalArgumentException("Unknown bridge mode: " + mode);
            }
        } finally {
            if (decompiler != null) {
                decompiler.dispose();
            }
        }
    }

    private void initDecompiler() {
        DecompileOptions options = new DecompileOptions();
        options.grabFromProgram(currentProgram);
        options.setMaxInstructions(Integer.MAX_VALUE);
        decompiler = new DecompInterface();
        decompiler.setOptions(options);
        decompiler.toggleCCode(true);
        decompiler.toggleSyntaxTree(true);
        decompiler.toggleJumpLoads(true);
        if (!decompiler.openProgram(currentProgram)) {
            throw new IllegalStateException("Ghidra decompiler could not open " + currentProgram.getName());
        }
    }

    private void runOneshot(JsonObject config) throws Exception {
        Path outputFile = Path.of(requireString(config, "output_file"));
        int timeoutSeconds = config.has("timeout_seconds") && !config.get("timeout_seconds").isJsonNull()
            ? config.get("timeout_seconds").getAsInt()
            : DEFAULT_TIMEOUT_SECONDS;
        JsonArray requests = config.getAsJsonArray("requests");
        JsonObject response = handleDecompileBatch(requests, timeoutSeconds);
        atomicWrite(outputFile, GSON.toJson(response) + "\n");
    }

    private void runDaemon(JsonObject config) throws Exception {
        Path serverFile = Path.of(requireString(config, "server_file"));
        String expectedToken = requireString(config, "token");
        long idleTimeoutMs = config.has("idle_timeout_ms") && !config.get("idle_timeout_ms").isJsonNull()
            ? config.get("idle_timeout_ms").getAsLong()
            : DEFAULT_IDLE_TIMEOUT_MS;
        long myPid = ProcessHandle.current().pid();
        byte[] expectedTokenBytes = expectedToken.getBytes(StandardCharsets.UTF_8);

        try (ServerSocket server = new ServerSocket(0, 16, InetAddress.getByName("127.0.0.1"))) {
            server.setSoTimeout(1000);

            JsonObject descriptor = new JsonObject();
            descriptor.addProperty("pid", myPid);
            descriptor.addProperty("port", server.getLocalPort());
            descriptor.addProperty("token", expectedToken);
            descriptor.addProperty("image_base_hex", imageBaseHex());
            descriptor.addProperty("program", currentProgram.getName());
            descriptor.addProperty("started_at_unix", Instant.now().getEpochSecond());
            atomicWrite(serverFile, GSON.toJson(descriptor) + "\n");
            println("HarmonyDecompileBridge daemon listening on 127.0.0.1:" + server.getLocalPort());

            long lastActivity = System.currentTimeMillis();
            boolean running = true;
            while (running && !monitor.isCancelled()) {
                Socket client;
                try {
                    client = server.accept();
                } catch (SocketTimeoutException timeout) {
                    if (System.currentTimeMillis() - lastActivity >= idleTimeoutMs) {
                        println("HarmonyDecompileBridge idle timeout reached; shutting down");
                        break;
                    }
                    continue;
                }

                try (Socket conn = client;
                     BufferedReader reader = new BufferedReader(new InputStreamReader(conn.getInputStream(), StandardCharsets.UTF_8));
                     BufferedWriter writer = new BufferedWriter(new OutputStreamWriter(conn.getOutputStream(), StandardCharsets.UTF_8))) {
                    conn.setSoTimeout(30_000);
                    String line = reader.readLine();
                    if (line == null || line.isBlank()) {
                        continue;
                    }
                    JsonObject req = JsonParser.parseString(line).getAsJsonObject();
                    String provided = req.has("token") && !req.get("token").isJsonNull() ? req.get("token").getAsString() : "";
                    if (!MessageDigest.isEqual(expectedTokenBytes, provided.getBytes(StandardCharsets.UTF_8))) {
                        JsonObject denied = new JsonObject();
                        denied.addProperty("ok", false);
                        denied.addProperty("error", "unauthorized");
                        writer.write(GSON.toJson(denied) + "\n");
                        writer.flush();
                        continue;
                    }

                    lastActivity = System.currentTimeMillis();
                    String op = req.has("op") && !req.get("op").isJsonNull() ? req.get("op").getAsString() : "";
                    JsonObject reply;
                    switch (op) {
                        case "ping" -> {
                            reply = new JsonObject();
                            reply.addProperty("ok", true);
                            reply.addProperty("pid", myPid);
                            reply.addProperty("program", currentProgram.getName());
                            reply.addProperty("image_base_hex", imageBaseHex());
                        }
                        case "shutdown" -> {
                            reply = new JsonObject();
                            reply.addProperty("ok", true);
                            running = false;
                        }
                        case "decompile" -> {
                            int timeoutSeconds = req.has("timeout_seconds") && !req.get("timeout_seconds").isJsonNull()
                                ? req.get("timeout_seconds").getAsInt()
                                : DEFAULT_TIMEOUT_SECONDS;
                            JsonArray requests = req.getAsJsonArray("requests");
                            reply = handleDecompileBatch(requests, timeoutSeconds);
                            lastActivity = System.currentTimeMillis();
                        }
                        default -> {
                            reply = new JsonObject();
                            reply.addProperty("ok", false);
                            reply.addProperty("error", "unknown op: " + op);
                        }
                    }
                    writer.write(GSON.toJson(reply) + "\n");
                    writer.flush();
                } catch (Exception clientError) {
                    println("HarmonyDecompileBridge request error: " + clientError.getMessage());
                }
            }
        } finally {
            removeServerFileIfOwned(serverFile, myPid);
        }
    }

    private JsonObject handleDecompileBatch(JsonArray requests, int timeoutSeconds) {
        JsonArray results = new JsonArray();
        if (requests != null) {
            for (JsonElement elem : requests) {
                JsonObject item = elem.getAsJsonObject();
                long rva = item.get("rva").getAsLong();
                Long pdataStart = item.has("pdata_start") && !item.get("pdata_start").isJsonNull()
                    ? item.get("pdata_start").getAsLong()
                    : null;
                results.add(decompileOne(rva, pdataStart, timeoutSeconds));
            }
        }
        JsonObject response = new JsonObject();
        response.addProperty("ok", true);
        response.addProperty("image_base_hex", imageBaseHex());
        response.add("results", results);
        return response;
    }

    private JsonObject decompileOne(long rva, Long pdataStart, int timeoutSeconds) {
        JsonObject out = new JsonObject();
        out.addProperty("requested_rva", rva);
        List<String> warnings = new ArrayList<>();
        try {
            Address imageBase = currentProgram.getImageBase();
            Address requestedAddr = imageBase.add(rva);
            MemoryBlock block = currentProgram.getMemory().getBlock(requestedAddr);
            if (block == null) {
                return failResult(rva, "RVA 0x" + Long.toHexString(rva) + " is outside mapped memory", warnings);
            }

            Function fn = resolveFunction(imageBase, requestedAddr, pdataStart, warnings);
            if (fn == null) {
                return failResult(rva, "no function found at or containing RVA 0x" + Long.toHexString(rva), warnings);
            }
            if (fn.isExternal() || fn.getBody().isEmpty()) {
                return failResult(rva, "function at 0x" + Long.toHexString(fn.getEntryPoint().subtract(imageBase)) + " is external or has an empty body", warnings);
            }

            DecompileResults res = decompiler.decompileFunction(fn, timeoutSeconds, new TaskMonitorAdapter());
            if (res.isTimedOut()) {
                return failResult(rva, "decompilation timed out after " + timeoutSeconds + "s", warnings);
            }
            if (res.isCancelled()) {
                return failResult(rva, "decompilation was cancelled", warnings);
            }
            if (!res.decompileCompleted()) {
                return failResult(rva, "decompilation failed: " + res.getErrorMessage(), warnings);
            }
            DecompiledFunction df = res.getDecompiledFunction();
            if (df == null || df.getC() == null) {
                return failResult(rva, "decompiler returned no C output", warnings);
            }

            long entryRva = fn.getEntryPoint().subtract(imageBase);
            long offset = requestedAddr.subtract(fn.getEntryPoint());
            String sig = df.getSignature();
            if (sig == null || sig.isBlank()) {
                sig = fn.getPrototypeString(false, true);
            }
            out.addProperty("ok", true);
            out.addProperty("entry_rva", entryRva);
            out.addProperty("offset_in_function", offset);
            out.addProperty("ghidra_name", fn.getName());
            out.addProperty("signature", sig);
            out.addProperty("c_code", df.getC());
            out.add("warnings", toJsonArray(warnings));
            return out;
        } catch (Exception e) {
            return failResult(rva, e.getMessage() == null ? e.toString() : e.getMessage(), warnings);
        }
    }

    private Function resolveFunction(Address imageBase, Address requestedAddr, Long pdataStart, List<String> warnings) {
        Function fn = getFunctionAt(requestedAddr);
        if (fn == null) {
            fn = getFunctionContaining(requestedAddr);
        }
        if (fn == null && pdataStart != null) {
            Address pdataAddr = imageBase.add(pdataStart.longValue());
            fn = getFunctionAt(pdataAddr);
            if (fn == null) {
                fn = getFunctionContaining(pdataAddr);
            }
        }
        if (fn != null) {
            return fn;
        }

        Address startAddr = pdataStart != null ? imageBase.add(pdataStart.longValue()) : requestedAddr;
        MemoryBlock startBlock = currentProgram.getMemory().getBlock(startAddr);
        if (startBlock == null || !startBlock.isExecute() || !startBlock.isInitialized()) {
            return null;
        }

        int tx = currentProgram.startTransaction("Harmony transient function recovery");
        boolean committed = false;
        try {
            if (getInstructionAt(startAddr) == null && getInstructionContaining(startAddr) == null && getDataAt(startAddr) == null) {
                disassemble(startAddr);
            }
            if (getInstructionAt(startAddr) != null) {
                fn = getFunctionAt(startAddr);
                if (fn == null) {
                    fn = createFunction(startAddr, null);
                }
                if (fn == null) {
                    fn = getFunctionContaining(startAddr);
                }
            }
            committed = (fn != null);
        } catch (Exception e) {
            warnings.add("transient function recovery failed: " + e.getMessage());
        } finally {
            currentProgram.endTransaction(tx, committed);
        }
        if (fn != null) {
            warnings.add("function was not pre-identified by auto-analysis; transiently decoded on demand");
        }
        return fn;
    }

    private JsonObject failResult(long rva, String error, List<String> warnings) {
        JsonObject out = new JsonObject();
        out.addProperty("ok", false);
        out.addProperty("requested_rva", rva);
        out.addProperty("error", error);
        out.add("warnings", toJsonArray(warnings));
        return out;
    }

    private JsonArray toJsonArray(List<String> items) {
        JsonArray arr = new JsonArray();
        for (String s : items) {
            arr.add(s);
        }
        return arr;
    }

    private String imageBaseHex() {
        return Long.toUnsignedString(currentProgram.getImageBase().getOffset(), 16);
    }

    private static String requireString(JsonObject obj, String key) {
        if (!obj.has(key) || obj.get(key).isJsonNull()) {
            throw new IllegalArgumentException("Missing required key: " + key);
        }
        return obj.get(key).getAsString();
    }

    private static void atomicWrite(Path path, String content) throws Exception {
        Files.createDirectories(path.getParent());
        Path tmp = path.resolveSibling(path.getFileName() + ".tmp");
        Files.writeString(tmp, content, StandardCharsets.UTF_8);
        try {
            Files.move(tmp, path, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (AtomicMoveNotSupportedException ignored) {
            Files.move(tmp, path, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private static void removeServerFileIfOwned(Path serverFile, long myPid) {
        try {
            if (!Files.isRegularFile(serverFile)) {
                return;
            }
            JsonObject current = JsonParser.parseString(Files.readString(serverFile, StandardCharsets.UTF_8)).getAsJsonObject();
            if (current.has("pid") && current.get("pid").getAsLong() == myPid) {
                Files.deleteIfExists(serverFile);
            }
        } catch (Exception ignored) {
        }
    }
}
