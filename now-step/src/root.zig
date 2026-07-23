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
pub const SecretCollection = @import("secret_collection.zig");

const std = @import("std");
const expect = std.testing.expect;

test "ignore empty secrets" {
    var arena: std.heap.ArenaAllocator = .init(std.heap.page_allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    var map = std.process.Environ.Map.init(allocator);
    defer map.deinit();
    try map.put("FOO", "Secret");
    try map.put("BAR", "");

    var secret_names: std.ArrayList([]const u8) = .empty;
    defer secret_names.deinit(allocator);
    try secret_names.append(allocator, "FOO");
    try secret_names.append(allocator, "BAR");

    var collection = try SecretCollection.init(allocator, &map, secret_names.items);
    defer collection.deinit();

    try expect(std.mem.eql(u8, try collection.redactLine("Regular"), "Regular"));
    try expect(std.mem.eql(u8, try collection.redactLine("Secret"), "***"));
}

test "anonymize" {
    var arena: std.heap.ArenaAllocator = .init(std.heap.page_allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    var map = std.process.Environ.Map.init(allocator);
    defer map.deinit();
    try map.put("ONE", "SECRET");
    try map.put("TWO", "ANOTHER_ONE");
    try map.put("THREE", "MORE_SECRET");
    try map.put("FOUR", "SECRET");

    var secret_names: std.ArrayList([]const u8) = .empty;
    defer secret_names.deinit(allocator);
    try secret_names.append(allocator, "ONE");
    try secret_names.append(allocator, "TWO");
    try secret_names.append(allocator, "THREE");
    try secret_names.append(allocator, "FOUR");

    var collection = try SecretCollection.init(allocator, &map, secret_names.items);
    defer collection.deinit();

    try expect(std.mem.eql(u8, try collection.redactLine("input"), "input"));
    try expect(std.mem.eql(u8, try collection.redactLine("SECRET"), "***"));
    try expect(std.mem.eql(u8, try collection.redactLine("123ANOTHER_ONE456MORE_SECRET789"), "123***456***789"));
}

test "multiple matches" {
    var arena: std.heap.ArenaAllocator = .init(std.heap.page_allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    var map = std.process.Environ.Map.init(allocator);
    defer map.deinit();
    try map.put("ABA", "aba");

    var secret_names: std.ArrayList([]const u8) = .empty;
    defer secret_names.deinit(allocator);
    try secret_names.append(allocator, "ABA");

    var collection = try SecretCollection.init(allocator, &map, secret_names.items);
    defer collection.deinit();

    try expect(std.mem.eql(u8, try collection.redactLine("123aba123ababa123"), "123***123***ba123"));
}
