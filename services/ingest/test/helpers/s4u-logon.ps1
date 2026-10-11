# SPEC-020 auth_ac_013 (Amendment 2026-10-11): an S4U network logon of the
# current user, for the logon marquee. One LsaLogonUser call through the
# MSV1_0 package, logon type 3, no password; the token it returns, at
# identification level, is closed at once. Prints only the status codes and
# exits 0 when the logon succeeded. Windows only, elevated.
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class CgS4u {
  [StructLayout(LayoutKind.Sequential)]
  public struct LSA_STRING { public ushort Length; public ushort MaximumLength; public IntPtr Buffer; }

  [StructLayout(LayoutKind.Sequential)]
  public struct LUID { public uint LowPart; public int HighPart; }

  [StructLayout(LayoutKind.Sequential)]
  public struct TOKEN_SOURCE {
    [MarshalAs(UnmanagedType.ByValArray, SizeConst = 8)] public byte[] SourceName;
    public LUID SourceIdentifier;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct QUOTA_LIMITS {
    public IntPtr PagedPoolLimit; public IntPtr NonPagedPoolLimit; public IntPtr MinimumWorkingSetSize;
    public IntPtr MaximumWorkingSetSize; public IntPtr PagefileLimit; public long TimeLimit;
  }

  [DllImport("secur32.dll")] static extern int LsaConnectUntrusted(out IntPtr handle);
  [DllImport("secur32.dll")] static extern int LsaLookupAuthenticationPackage(IntPtr handle, ref LSA_STRING name, out uint package);
  [DllImport("secur32.dll")] static extern int LsaLogonUser(IntPtr handle, ref LSA_STRING origin, int logonType, uint package,
      IntPtr authInfo, uint authInfoLength, IntPtr localGroups, ref TOKEN_SOURCE source,
      out IntPtr profile, out uint profileLength, out LUID logonId, out IntPtr token, out QUOTA_LIMITS quotas, out int subStatus);
  [DllImport("secur32.dll")] static extern int LsaFreeReturnBuffer(IntPtr buffer);
  [DllImport("secur32.dll")] static extern int LsaDeregisterLogonProcess(IntPtr handle);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);

  static LSA_STRING Ansi(string s, out IntPtr mem) {
    byte[] b = Encoding.ASCII.GetBytes(s);
    mem = Marshal.AllocHGlobal(b.Length + 1);
    Marshal.Copy(b, 0, mem, b.Length);
    Marshal.WriteByte(mem, b.Length, 0);
    LSA_STRING r;
    r.Length = (ushort)b.Length; r.MaximumLength = (ushort)(b.Length + 1); r.Buffer = mem;
    return r;
  }

  // The NTSTATUS of the call that failed, or 0; the token is closed.
  public static int Logon(string user, string domain) {
    IntPtr lsa;
    int st = LsaConnectUntrusted(out lsa);
    if (st != 0) return st;
    IntPtr pkgMem, originMem;
    LSA_STRING pkgName = Ansi("MICROSOFT_AUTHENTICATION_PACKAGE_V1_0", out pkgMem);
    LSA_STRING origin = Ansi("CgGate", out originMem);
    uint pkg;
    st = LsaLookupAuthenticationPackage(lsa, ref pkgName, out pkg);
    if (st != 0) {
      Marshal.FreeHGlobal(pkgMem); Marshal.FreeHGlobal(originMem);
      LsaDeregisterLogonProcess(lsa);
      return st;
    }

    // MSV1_0_S4U_LOGON: MessageType, Flags, UserPrincipalName, DomainName;
    // a UNICODE_STRING is 2 * pointer size; the strings follow the header,
    // and the members point inside the same buffer.
    byte[] u = Encoding.Unicode.GetBytes(user);
    byte[] d = Encoding.Unicode.GetBytes(domain);
    int us = 8, ds = 8 + 2 * IntPtr.Size, head = 8 + 4 * IntPtr.Size;
    int total = head + u.Length + d.Length;
    IntPtr buf = Marshal.AllocHGlobal(total);
    for (int i = 0; i < total; i++) Marshal.WriteByte(buf, i, 0);
    IntPtr uPtr = new IntPtr(buf.ToInt64() + head);
    IntPtr dPtr = new IntPtr(buf.ToInt64() + head + u.Length);
    Marshal.Copy(u, 0, uPtr, u.Length);
    Marshal.Copy(d, 0, dPtr, d.Length);
    Marshal.WriteInt32(buf, 0, 12); // MsV1_0S4ULogon
    Marshal.WriteInt32(buf, 4, 0);
    Marshal.WriteInt16(buf, us, (short)u.Length); Marshal.WriteInt16(buf, us + 2, (short)u.Length);
    Marshal.WriteIntPtr(buf, us + IntPtr.Size, uPtr);
    Marshal.WriteInt16(buf, ds, (short)d.Length); Marshal.WriteInt16(buf, ds + 2, (short)d.Length);
    Marshal.WriteIntPtr(buf, ds + IntPtr.Size, dPtr);

    TOKEN_SOURCE src;
    src.SourceName = new byte[8];
    Encoding.ASCII.GetBytes("CgGate").CopyTo(src.SourceName, 0);
    src.SourceIdentifier = new LUID();
    IntPtr profile, token; uint plen; LUID lid; QUOTA_LIMITS q; int sub;
    st = LsaLogonUser(lsa, ref origin, 3, pkg, buf, (uint)total, IntPtr.Zero, ref src,
        out profile, out plen, out lid, out token, out q, out sub);
    if (profile != IntPtr.Zero) LsaFreeReturnBuffer(profile);
    if (token != IntPtr.Zero) CloseHandle(token);
    Marshal.FreeHGlobal(buf); Marshal.FreeHGlobal(pkgMem); Marshal.FreeHGlobal(originMem);
    LsaDeregisterLogonProcess(lsa);
    return st;
  }
}
'@

$status = [CgS4u]::Logon($env:USERNAME, $env:COMPUTERNAME)
Write-Output ("s4u_logon_status=0x{0:x8}" -f $status)
if ($status -ne 0) { exit 1 }
exit 0
