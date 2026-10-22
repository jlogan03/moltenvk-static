# Upstream work

Issues to submit to MoltenVK and its dependencies.

## 1. Register the swapchain alias when Vulkan symbols are hidden

MoltenVK version 1.4.2, pinned at
[`db66022459ffb663aa2b50f6b018bc2e124f5edf`](https://github.com/KhronosGroup/MoltenVK/tree/db66022459ffb663aa2b50f6b018bc2e124f5edf).

With `MVK_HIDE_VULKAN_SYMBOLS=1`, MoltenVK omits the exported
`vkReleaseSwapchainImagesEXT` alias, but its procedure table still references
that symbol. Linking fails with an undefined symbol.

## 2. Prevent timeline semaphore callbacks from waking a reused waiter

MoltenVK version 1.4.2, pinned at
[`db66022459ffb663aa2b50f6b018bc2e124f5edf`](https://github.com/KhronosGroup/MoltenVK/tree/db66022459ffb663aa2b50f6b018bc2e124f5edf).

This is a confirmed data race.

[`mvkWaitSemaphores`](https://github.com/KhronosGroup/MoltenVK/blob/db66022459ffb663aa2b50f6b018bc2e124f5edf/MoltenVK/MoltenVK/GPUObjects/MVKSync.mm#L401-L435)
creates a waiter on the stack.
[`registerWait`](https://github.com/KhronosGroup/MoltenVK/blob/db66022459ffb663aa2b50f6b018bc2e124f5edf/MoltenVK/MoltenVK/GPUObjects/MVKSync.mm#L210-L234)
registers a Metal callback that identifies the waiter by its address. A timed-out
wait removes that address from the active set, but the callback can run later.

The failure sequence is:

1. A wait for value A times out, leaving its callback pending.
2. A wait for a higher value B reuses the same stack address.
3. The callback for A finds that address in the active set and signals the new
   waiter, even though the semaphore has not reached B.

Zero-timeout polling can trigger this sequence. Early completion could let a
consumer read or reuse buffers while the GPU still uses them.

## 3. Preserve code after collapsed selections in SPIRV-Cross

SPIRV-Cross revision
[`6c09849fe88c48eaed08413aa022aaa136a3a057`](https://github.com/KhronosGroup/SPIRV-Cross/tree/6c09849fe88c48eaed08413aa022aaa136a3a057),
used by MoltenVK 1.4.2.

When both targets of `OpBranchConditional` are its selection merge block, the
parser replaces the conditional with a direct branch. It leaves the target
marked as a selection merge, so code generation skips that block. In krnl's
conversions to f16, this drops assignments and leaves the output uninitialized.

[collapsed-selection.patch](build-support/patches/collapsed-selection.patch)
clears the stale marker. Translating krnl's f32-to-f16 shader with the patch
restores the three missing assignments.

The [minimal reproduction](build-support/patches/collapsed-selection.spvasm)
passes SPIRV-Tools validation for Vulkan 1.1. It must write `1` to its buffer.
Unpatched SPIRV-Cross emits an empty Metal kernel; the patch restores the store.
Validation on Metal is pending.
