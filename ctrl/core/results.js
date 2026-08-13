export const ResultStatus = Object.freeze({
  SUCCESS: "success",
  INVALID_INPUT: "invalid_input",
  INVALID_CENTRAL_STRUCTURE: "invalid_central_structure",
  UNAVAILABLE_CAPABILITY: "unavailable_capability",
  CONNECTOR_FAILURE: "connector_failure",
  INTERNAL_FAILURE: "internal_failure",
});

export function success(action, data) {
  return { ok: true, status: ResultStatus.SUCCESS, action, data };
}

export function failure(action, status, message, details = undefined) {
  return {
    ok: false,
    status,
    action,
    error: {
      code: status,
      message,
      ...(details === undefined ? {} : { details }),
    },
  };
}
