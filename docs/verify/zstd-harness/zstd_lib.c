/* Single-translation-unit wrapper for pinned facebook/zstd sources (BSD-3-Clause).
 * Mirrors upstream build/single_file_libs/zstd-in.c with ZSTD_LEGACY_SUPPORT=0,
 * ZSTD_DISABLE_ASM=1, and single-threaded execution (no ZSTD_MULTITHREAD). */
#define DEBUGLEVEL 0
#define MEM_MODULE
#undef XXH_NAMESPACE
#define XXH_NAMESPACE ZSTD_
#undef XXH_PRIVATE_API
#define XXH_PRIVATE_API
#undef XXH_INLINE_ALL
#define XXH_INLINE_ALL
#define ZSTD_LEGACY_SUPPORT 0
#define ZSTD_TRACE 0
#define ZSTD_DISABLE_ASM 1
#define ZSTD_DEPS_NEED_MALLOC
#define ZSTD_DEPS_NEED_MATH64
#include "common/zstd_deps.h"

#include "common/debug.inc"
#include "common/entropy_common.inc"
#include "common/error_private.inc"
#include "common/fse_decompress.inc"
#include "common/threading.inc"
#include "common/pool.inc"
#include "common/zstd_common.inc"

#include "compress/fse_compress.inc"
#include "compress/hist.inc"
#include "compress/huf_compress.inc"
#include "compress/zstd_compress_literals.inc"
#include "compress/zstd_compress_sequences.inc"
#include "compress/zstd_compress_superblock.inc"
#include "compress/zstd_preSplit.inc"
#include "compress/zstd_compress.inc"
#include "compress/zstd_double_fast.inc"
#include "compress/zstd_fast.inc"
#include "compress/zstd_lazy.inc"
#include "compress/zstd_ldm.inc"
#include "compress/zstd_opt.inc"

#include "decompress/huf_decompress.inc"
#include "decompress/zstd_ddict.inc"
#include "decompress/zstd_decompress.inc"
#include "decompress/zstd_decompress_block.inc"
