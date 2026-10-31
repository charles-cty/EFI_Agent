/* vi: set sw=4 ts=4: */
/*
 * Licensed under GPLv2 or later, see file LICENSE in this source tree.
 */
#include "libbb.h"
#include "bb_archive.h"

void FAST_FUNC init_transformer_state(transformer_state_t *xstate)
{
	memset(xstate, 0, sizeof(*xstate));
}

int FAST_FUNC check_signature16(transformer_state_t *xstate, unsigned magic16)
{
	if (!xstate->signature_skipped) {
		uint16_t magic2;
		if (full_read(xstate->src_fd, &magic2, 2) != 2 || magic2 != magic16) {
			bb_simple_error_msg("invalid magic");
			return -1;
		}
		xstate->signature_skipped = 2;
	}
	return 0;
}

ssize_t FAST_FUNC transformer_write(transformer_state_t *xstate, const void *buf, size_t bufsize)
{
	ssize_t nwrote;

	if (xstate->mem_output_size_max != 0) {
		size_t pos = xstate->mem_output_size;
		size_t size;

		size = (xstate->mem_output_size += bufsize);
		if (size > xstate->mem_output_size_max) {
			free(xstate->mem_output_buf);
			xstate->mem_output_buf = NULL;
			bb_perror_msg("buffer %u too small", (unsigned)xstate->mem_output_size_max);
			nwrote = -1;
			goto ret;
		}
		xstate->mem_output_buf = xrealloc(xstate->mem_output_buf, size + 1);
		memcpy(xstate->mem_output_buf + pos, buf, bufsize);
		xstate->mem_output_buf[size] = '\0';
		nwrote = bufsize;
	} else {
		nwrote = full_write(xstate->dst_fd, buf, bufsize);
		if (nwrote != (ssize_t)bufsize) {
			bb_simple_perror_msg("write");
			nwrote = -1;
			goto ret;
		}
	}
 ret:
	return nwrote;
}

ssize_t FAST_FUNC xtransformer_write(transformer_state_t *xstate, const void *buf, size_t bufsize)
{
	ssize_t nwrote = transformer_write(xstate, buf, bufsize);
	if (nwrote != (ssize_t)bufsize) {
		xfunc_die();
	}
	return nwrote;
}
