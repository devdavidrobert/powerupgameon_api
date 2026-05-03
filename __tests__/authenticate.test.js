jest.mock("../config/firebase", () => ({
  getAuth: jest.fn(),
}));

const { getAuth } = require("../config/firebase");
const { authenticate } = require("../middleware/authenticate");

describe("authenticate middleware", () => {
  let mockRes;
  let mockNext;

  beforeEach(() => {
    mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() };
    mockNext = jest.fn();
    getAuth.mockReset();
  });

  it("sets req.user.admin from Firebase custom claim", async () => {
    getAuth.mockReturnValue({
      verifyIdToken: jest.fn().mockResolvedValue({
        uid: "u1",
        email: "a@b.com",
        admin: true,
      }),
    });

    const req = { headers: { authorization: "Bearer fake.jwt.token" } };
    await authenticate(req, mockRes, mockNext);

    expect(mockNext).toHaveBeenCalled();
    expect(req.user).toEqual({ uid: "u1", email: "a@b.com", admin: true });
  });

  it("sets admin false when claim absent", async () => {
    getAuth.mockReturnValue({
      verifyIdToken: jest.fn().mockResolvedValue({
        uid: "u1",
        email: "a@b.com",
      }),
    });

    const req = { headers: { authorization: "Bearer fake.jwt.token" } };
    await authenticate(req, mockRes, mockNext);

    expect(req.user).toEqual({ uid: "u1", email: "a@b.com", admin: false });
  });

  it("returns 401 when Bearer missing", async () => {
    const req = { headers: {} };
    await authenticate(req, mockRes, mockNext);
    expect(mockRes.status).toHaveBeenCalledWith(401);
    expect(mockNext).not.toHaveBeenCalled();
  });
});
