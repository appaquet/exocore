#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

enum ExocoreError {
  ExocoreError_Success = 0,
  ExocoreError_TokioRuntime = 1,
};
typedef uint8_t ExocoreError;

typedef struct ExocoreContext ExocoreContext;

typedef struct ExocoreContextResult {
  ExocoreError status;
  ExocoreContext *context;
} ExocoreContextResult;

ExocoreContextResult exocore_context_new(void);

void exocore_send_query(ExocoreContext *ctx, const char *query);
