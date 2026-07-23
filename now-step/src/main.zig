// now: A Nix-based distributed command runner
// Copyright (C) 2026 Eric Rodrigues Pires
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for
// more details.
//
// You should have received a copy of the GNU Affero General Public License along
// with this program. If not, see <https://www.gnu.org/licenses/>.

const std = @import("std");
const builtin = @import("builtin");

const now_step = @import("now_step");
const pty = @import("zigpty");
const zigcli = @import("zigcli");

pub fn main(init: std.process.Init) !void {
    const allocator: std.mem.Allocator = init.arena.allocator();

    var opt = try zigcli.structargs.parse(allocator, init.io, init.minimal.args, struct {
        derivation: []const u8,
        secrets: []const u8,
    }, .{});
    defer opt.deinit();

    const parsed_secrets = try std.json.parseFromSlice([][]const u8, allocator, opt.options.secrets, .{});
    defer parsed_secrets.deinit();
    const secret_names = parsed_secrets.value;

    var secret_collection = try now_step.SecretCollection.init(allocator, init.environ_map, secret_names);
    defer secret_collection.deinit();

    const file = try std.fmt.allocPrintSentinel(allocator, "{s}", .{opt.options.derivation}, 0);
    const cwd = try std.Io.Dir.cwd().realPathFileAlloc(init.io, ".", allocator);

    const result = try pty.forkPty(.{
        .file = file,
        .argv = &.{null},
        .envp = init.minimal.environ.block.slice,
        .cwd = cwd,
        .cols = 120,
        .rows = 30,
    });

    var pty_buffer: [2048]u8 = undefined;
    var pty_file = std.Io.File{ .handle = result.fd, .flags = .{ .nonblocking = false } };
    var file_reader = pty_file.reader(init.io, &pty_buffer);
    const r = &file_reader.interface;

    var stdout_buffer: [2048]u8 = undefined;
    var stdout_file_writer: std.Io.File.Writer = .init(.stdout(), init.io, &stdout_buffer);
    const stdout_writer = &stdout_file_writer.interface;

    while (true) {
        const line = r.takeDelimiterInclusive('\n') catch |err| switch (err) {
            error.EndOfStream => {
                const rest = r.buffered();
                if (rest.len > 0) {
                    const redacted = try secret_collection.redactLine(rest);
                    try stdout_writer.writeAll(redacted);
                }
                break;
            },
            error.ReadFailed => {
                if (file_reader.err) |e| switch (e) {
                    error.InputOutput => {
                        const rest = r.buffered();
                        if (rest.len > 0) {
                            const redacted = try secret_collection.redactLine(rest);
                            try stdout_writer.writeAll(redacted);
                        }
                        break;
                    },
                    else => return e,
                };
                break;
            },
            else => return err,
        };

        const redacted_line = try secret_collection.redactLine(line);
        try stdout_writer.writeAll(redacted_line);
        try stdout_writer.flush();
    }
    try stdout_writer.flush();

    const exit_info = pty.waitForExit(result.pid);
    std.process.exit(@intCast(exit_info.exit_code));
}
