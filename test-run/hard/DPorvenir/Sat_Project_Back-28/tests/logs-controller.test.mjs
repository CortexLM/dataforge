import { jest } from '@jest/globals';

const inputCalls = [];
const queryMock = jest.fn();
const request = {
  input: jest.fn((name, type, value) => {
    inputCalls.push({ name, type, value });
    return request;
  }),
  query: queryMock
};
const requestMock = jest.fn(() => request);

await jest.unstable_mockModule('../src/config/DbConfig.mjs', () => ({
  poolPromise: Promise.resolve({ request: requestMock }),
  sql: { Int: 'IntType' }
}));

const { listarLogsController } = await import('../src/controllers/LogsController.mjs');

function buildRes() {
  const res = {};
  res.status = jest.fn(() => res);
  res.json = jest.fn(() => res);
  return res;
}

beforeEach(() => {
  inputCalls.length = 0;
  queryMock.mockReset();
  requestMock.mockClear();
  request.input.mockClear();
});

test('listarLogsController rejects unauthenticated users', async () => {
  const req = { usuario: null, query: {} };
  const res = buildRes();

  await listarLogsController(req, res);

  expect(res.status).toHaveBeenCalledWith(401);
  expect(res.json).toHaveBeenCalledWith({ error: 'Usuario no autenticado' });
  expect(queryMock).not.toHaveBeenCalled();
});

test('listarLogsController clamps limit to max and returns logs', async () => {
  const req = { usuario: { usuarioId: 42 }, query: { limit: '500' } };
  const res = buildRes();
  const fakeLogs = [{ LogId: 1 }, { LogId: 2 }];
  queryMock.mockResolvedValue({ recordset: fakeLogs });

  await listarLogsController(req, res);

  expect(inputCalls).toContainEqual({ name: 'UsuarioId', type: 'IntType', value: 42 });
  expect(inputCalls).toContainEqual({ name: 'Limit', type: 'IntType', value: 200 });
  expect(res.json).toHaveBeenCalledWith({ ok: true, limit: 200, logs: fakeLogs });
});

test('listarLogsController normalizes invalid and low limits', async () => {
  const res = buildRes();
  queryMock.mockResolvedValue({ recordset: [] });

  const reqLow = { usuario: { usuarioId: 7 }, query: { limit: '0' } };
  await listarLogsController(reqLow, res);
  expect(inputCalls).toContainEqual({ name: 'Limit', type: 'IntType', value: 1 });

  inputCalls.length = 0;
  const reqInvalid = { usuario: { usuarioId: 7 }, query: { limit: 'abc' } };
  await listarLogsController(reqInvalid, res);
  expect(inputCalls).toContainEqual({ name: 'Limit', type: 'IntType', value: 10 });
});
