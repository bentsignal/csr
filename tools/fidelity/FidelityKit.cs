// Helpers for tools/fidelity/fidelity.ps1: a CS:GO net console client,
// window capture and image comparison. Loaded with Add-Type; no
// dependencies beyond System.Drawing.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace CsrFidelity
{
    /// Line-based client for the game's `-netconport` console. Everything
    /// the console prints is collected with a receive timestamp, so sound
    /// starts (`snd_showstart`) can be timed.
    public class NetCon : IDisposable
    {
        TcpClient client;
        NetworkStream stream;
        Thread reader;
        readonly List<KeyValuePair<double, string>> lines = new List<KeyValuePair<double, string>>();
        readonly Stopwatch clock = Stopwatch.StartNew();

        public static NetCon Connect(int port, int timeoutMs)
        {
            var deadline = DateTime.UtcNow.AddMilliseconds(timeoutMs);
            while (true)
            {
                try
                {
                    var c = new TcpClient();
                    c.NoDelay = true;
                    c.Connect("127.0.0.1", port);
                    var n = new NetCon { client = c, stream = c.GetStream() };
                    n.reader = new Thread(n.Read) { IsBackground = true };
                    n.reader.Start();
                    return n;
                }
                catch (SocketException)
                {
                    if (DateTime.UtcNow > deadline) throw;
                    Thread.Sleep(500);
                }
            }
        }

        void Read()
        {
            var buf = new byte[65536];
            var pending = new StringBuilder();
            try
            {
                while (true)
                {
                    int n = stream.Read(buf, 0, buf.Length);
                    if (n <= 0) break;
                    double t = clock.Elapsed.TotalSeconds;
                    pending.Append(Encoding.UTF8.GetString(buf, 0, n));
                    string s = pending.ToString();
                    int nl;
                    while ((nl = s.IndexOf('\n')) >= 0)
                    {
                        string line = s.Substring(0, nl).TrimEnd('\r');
                        lock (lines) lines.Add(new KeyValuePair<double, string>(t, line));
                        s = s.Substring(nl + 1);
                    }
                    pending.Clear();
                    pending.Append(s);
                }
            }
            catch (IOException) { }
            catch (ObjectDisposedException) { }
        }

        public double Now { get { return clock.Elapsed.TotalSeconds; } }

        public void Send(string command)
        {
            var bytes = Encoding.UTF8.GetBytes(command + "\n");
            stream.Write(bytes, 0, bytes.Length);
        }

        /// Lines received at or after `since` (seconds on this client's clock).
        public string[] Since(double since)
        {
            var o = new List<string>();
            lock (lines)
                foreach (var l in lines)
                    if (l.Key >= since) o.Add(l.Key.ToString("F3") + "\t" + l.Value);
            return o.ToArray();
        }

        /// Waits until a line containing `needle` arrives after `since`.
        public bool WaitFor(string needle, double since, int timeoutMs)
        {
            var deadline = DateTime.UtcNow.AddMilliseconds(timeoutMs);
            while (DateTime.UtcNow < deadline)
            {
                lock (lines)
                    foreach (var l in lines)
                        if (l.Key >= since && l.Value.Contains(needle)) return true;
                Thread.Sleep(100);
            }
            return false;
        }

        public void Dispose()
        {
            try { client.Close(); } catch { }
        }
    }

    public static class Window
    {
        [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
        [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr hwnd, out RECT rect);
        [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd, ref POINT p);
        [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
        [StructLayout(LayoutKind.Sequential)] struct RECT { public int Left, Top, Right, Bottom; }
        [StructLayout(LayoutKind.Sequential)] struct POINT { public int X, Y; }

        /// Captures the client area. PrintWindow with PW_RENDERFULLCONTENT
        /// reads the DWM surface (works for D3D9 windowed games); if that
        /// comes back black the visible screen pixels are copied instead.
        public static void Capture(IntPtr hwnd, string path)
        {
            RECT wr, cr;
            GetWindowRect(hwnd, out wr);
            GetClientRect(hwnd, out cr);
            var origin = new POINT();
            ClientToScreen(hwnd, ref origin);
            int w = cr.Right - cr.Left, h = cr.Bottom - cr.Top;
            using (var whole = new Bitmap(wr.Right - wr.Left, wr.Bottom - wr.Top, PixelFormat.Format32bppArgb))
            {
                using (var g = Graphics.FromImage(whole))
                {
                    IntPtr hdc = g.GetHdc();
                    PrintWindow(hwnd, hdc, 2);
                    g.ReleaseHdc(hdc);
                }
                var client = new Rectangle(origin.X - wr.Left, origin.Y - wr.Top, w, h);
                using (var crop = whole.Clone(client, PixelFormat.Format24bppRgb))
                {
                    if (MeanLuma(crop) < 0.5)
                    {
                        SetForegroundWindow(hwnd);
                        Thread.Sleep(400);
                        using (var g = Graphics.FromImage(crop))
                            g.CopyFromScreen(origin.X, origin.Y, 0, 0, new Size(w, h));
                    }
                    crop.Save(path, ImageFormat.Png);
                }
            }
        }

        static double MeanLuma(Bitmap b)
        {
            double sum = 0; int n = 0;
            for (int y = 0; y < b.Height; y += 16)
                for (int x = 0; x < b.Width; x += 16) { var c = b.GetPixel(x, y); sum += c.R + c.G + c.B; n++; }
            return sum / Math.Max(1, n) / 3.0;
        }
    }

    /// Image comparison: brightness, colour and structure in one row.
    public static class Compare
    {
        static float[] Load(string path, out int w, out int h)
        {
            using (var src = new Bitmap(path))
            using (var b = src.Clone(new Rectangle(0, 0, src.Width, src.Height), PixelFormat.Format24bppRgb))
            {
                w = b.Width; h = b.Height;
                var data = b.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.ReadOnly, PixelFormat.Format24bppRgb);
                var raw = new byte[data.Stride * h];
                Marshal.Copy(data.Scan0, raw, 0, raw.Length);
                b.UnlockBits(data);
                var o = new float[w * h * 3];
                for (int y = 0; y < h; y++)
                    for (int x = 0; x < w; x++)
                    {
                        int s = y * data.Stride + x * 3, d = (y * w + x) * 3;
                        o[d] = raw[s + 2]; o[d + 1] = raw[s + 1]; o[d + 2] = raw[s];
                    }
                return o;
            }
        }

        static float Luma(float[] p, int i) { return 0.2126f * p[i] + 0.7152f * p[i + 1] + 0.0722f * p[i + 2]; }

        /// Returns "key=value" pairs. Values are in 0-255 sRGB units except
        /// SSIM (0-1, 1 = identical structure).
        public static string Metrics(string reference, string ours)
        {
            int w, h, w2, h2;
            var a = Load(reference, out w, out h);
            var b = Load(ours, out w2, out h2);
            if (w != w2 || h != h2) return "error=size_mismatch(" + w + "x" + h + " vs " + w2 + "x" + h2 + ")";
            int n = w * h;
            double[] meanA = new double[3], meanB = new double[3];
            double mae = 0;
            var lumA = new float[n]; var lumB = new float[n];
            for (int i = 0; i < n; i++)
            {
                for (int c = 0; c < 3; c++) { meanA[c] += a[i * 3 + c]; meanB[c] += b[i * 3 + c]; mae += Math.Abs(a[i * 3 + c] - b[i * 3 + c]); }
                lumA[i] = Luma(a, i * 3); lumB[i] = Luma(b, i * 3);
            }
            for (int c = 0; c < 3; c++) { meanA[c] /= n; meanB[c] /= n; }
            mae /= n * 3.0;
            // Block SSIM on luma, 8x8 non-overlapping windows.
            double ssim = 0; int blocks = 0;
            const double C1 = 6.5025, C2 = 58.5225;
            for (int by = 0; by + 8 <= h; by += 8)
                for (int bx = 0; bx + 8 <= w; bx += 8)
                {
                    double ma = 0, mb = 0;
                    for (int y = 0; y < 8; y++) for (int x = 0; x < 8; x++) { int i = (by + y) * w + bx + x; ma += lumA[i]; mb += lumB[i]; }
                    ma /= 64; mb /= 64;
                    double va = 0, vb = 0, cov = 0;
                    for (int y = 0; y < 8; y++) for (int x = 0; x < 8; x++)
                        {
                            int i = (by + y) * w + bx + x;
                            double da = lumA[i] - ma, db = lumB[i] - mb;
                            va += da * da; vb += db * db; cov += da * db;
                        }
                    va /= 63; vb /= 63; cov /= 63;
                    ssim += ((2 * ma * mb + C1) * (2 * cov + C2)) / ((ma * ma + mb * mb + C1) * (va + vb + C2));
                    blocks++;
                }
            ssim /= Math.Max(1, blocks);
            var sa = (float[])lumA.Clone(); Array.Sort(sa);
            var sb = (float[])lumB.Clone(); Array.Sort(sb);
            Func<float[], double, float> pct = (s, p) => s[(int)((s.Length - 1) * p)];
            var inv = System.Globalization.CultureInfo.InvariantCulture;
            Func<double, string> f = v => v.ToString("F1", inv);
            return string.Join(" ", new[] {
                "mae=" + f(mae),
                "ssim=" + ssim.ToString("F3", inv),
                "luma_ref=" + f(0.2126 * meanA[0] + 0.7152 * meanA[1] + 0.0722 * meanA[2]),
                "luma_ours=" + f(0.2126 * meanB[0] + 0.7152 * meanB[1] + 0.0722 * meanB[2]),
                "rgb_ref=" + f(meanA[0]) + "/" + f(meanA[1]) + "/" + f(meanA[2]),
                "rgb_ours=" + f(meanB[0]) + "/" + f(meanB[1]) + "/" + f(meanB[2]),
                "p05=" + f(pct(sa, 0.05)) + "/" + f(pct(sb, 0.05)),
                "p50=" + f(pct(sa, 0.50)) + "/" + f(pct(sb, 0.50)),
                "p95=" + f(pct(sa, 0.95)) + "/" + f(pct(sb, 0.95)),
            });
        }

        /// Writes a false-colour difference image (brighter = larger error).
        public static void DiffImage(string reference, string ours, string path)
        {
            int w, h, w2, h2;
            var a = Load(reference, out w, out h);
            var b = Load(ours, out w2, out h2);
            if (w != w2 || h != h2) return;
            using (var o = new Bitmap(w, h, PixelFormat.Format24bppRgb))
            {
                for (int y = 0; y < h; y++)
                    for (int x = 0; x < w; x++)
                    {
                        int i = (y * w + x) * 3;
                        int r = (int)Math.Min(255, Math.Abs(a[i] - b[i]) * 2), g = (int)Math.Min(255, Math.Abs(a[i + 1] - b[i + 1]) * 2), bl = (int)Math.Min(255, Math.Abs(a[i + 2] - b[i + 2]) * 2);
                        o.SetPixel(x, y, Color.FromArgb(r, g, bl));
                    }
                o.Save(path, ImageFormat.Png);
            }
        }
    }
}
