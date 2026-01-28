const std = @import("std");

pub fn build(b: *std.Build) void {
    // WASM target for plugin
    const target = b.resolveTargetQuery(.{
        .cpu_arch = .wasm32,
        .os_tag = .freestanding,
    });

    const optimize = b.standardOptimizeOption(.{});

    const exe = b.addExecutable(.{
        .name = "echo_plugin",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });

    // Export as dynamic library (WASM)
    exe.rdynamic = true;
    exe.entry = .disabled;

    // Install the artifact
    b.installArtifact(exe);

    // Add a run step for convenience
    const install_step = b.addInstallArtifact(exe, .{});
    const run_step = b.step("wasm", "Build WASM plugin");
    run_step.dependOn(&install_step.step);
}
