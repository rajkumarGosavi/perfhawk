// dotnet-demo: a small "order service" simulation with deliberate hot spots,
// meant to be profiled with dotnet-trace and opened in perfhawk.
//
//   dotnet run -c Release -- [cpu|alloc|lock|io|all] [seconds]
//
// See README.md (or ./profile.ps1 / ./profile.sh) for the capture commands.
using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace DotnetDemo;

public sealed record Item(string Sku, int Qty, double Price);
public sealed record Order(int Id, string Customer, List<Item> Items, DateTime Created);

public static class Program
{
    public static int Main(string[] args)
    {
        var mode = args.Length > 0 ? args[0].ToLowerInvariant() : "all";
        var seconds = args.Length > 1 && int.TryParse(args[1], out var s) ? s : 10;
        var valid = new[] { "cpu", "alloc", "lock", "io", "all" };
        if (!valid.Contains(mode))
        {
            Console.Error.WriteLine($"usage: dotnet-demo [{string.Join('|', valid)}] [seconds]");
            return 2;
        }

        Console.WriteLine($"pid {Environment.ProcessId}: running '{mode}' workload for {seconds}s ...");
        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(seconds));
        var svc = new OrderService();
        var tasks = new List<Task>();

        if (mode is "cpu" or "all")
            for (var i = 0; i < Math.Max(2, Environment.ProcessorCount / 2); i++)
            {
                var seed = i;
                tasks.Add(Task.Factory.StartNew(() => svc.HandleRequests(seed, cts.Token), TaskCreationOptions.LongRunning));
            }

        if (mode is "alloc" or "all")
            tasks.Add(Task.Factory.StartNew(() => svc.BuildReports(cts.Token), TaskCreationOptions.LongRunning));

        if (mode is "lock" or "all")
            for (var i = 0; i < 8; i++)
                tasks.Add(Task.Factory.StartNew(() => svc.ReserveInventory(cts.Token), TaskCreationOptions.LongRunning));

        if (mode is "io" or "all")
            tasks.Add(Task.Factory.StartNew(() => svc.WriteAuditLog(cts.Token), TaskCreationOptions.LongRunning));

        Task.WaitAll(tasks.ToArray());
        Console.WriteLine($"done: {svc.Summary()}");
        return 0;
    }
}

public sealed class OrderService
{
    private readonly object _inventoryLock = new();
    private readonly Dictionary<string, int> _inventory = new();
    private readonly List<byte[]> _cache = new(); // grows: the "leak" an alloc profile shows
    private long _handled;

    public OrderService()
    {
        for (var i = 0; i < 1000; i++) _inventory[$"sku-{i:D4}"] = 1_000_000;
    }

    // CPU: serialize, hash, sort.
    public void HandleRequests(int seed, CancellationToken ct)
    {
        var rng = new Random(seed);
        var id = seed * 1_000_000;
        while (!ct.IsCancellationRequested)
        {
            var order = RandomOrder(rng, id++);
            var body = Encode(order);
            var sum = Checksum(body);
            RankItems(order.Items);
            Remember(id, body, sum);
            Interlocked.Increment(ref _handled);
        }
    }

    private static Order RandomOrder(Random rng, int id)
    {
        var items = new List<Item>(5 + rng.Next(40));
        for (var i = 0; i < items.Capacity; i++)
            items.Add(new Item($"sku-{rng.Next(1000):D4}", 1 + rng.Next(5), rng.NextDouble() * 100));
        return new Order(id, $"customer-{rng.Next(10_000)}", items, DateTime.UtcNow);
    }

    private static byte[] Encode(Order order) => JsonSerializer.SerializeToUtf8Bytes(order);

    private static byte[] Checksum(byte[] body)
    {
        var sum = SHA256.HashData(body);
        for (var i = 0; i < 20; i++) sum = SHA256.HashData(sum); // deliberately wasteful
        return sum;
    }

    private static void RankItems(List<Item> items) =>
        items.Sort((a, b) => (b.Price * b.Qty).CompareTo(a.Price * a.Qty));

    private void Remember(int id, byte[] body, byte[] sum)
    {
        if (id % 100 != 0) return;
        var entry = new byte[body.Length + sum.Length];
        body.CopyTo(entry, 0);
        sum.CopyTo(entry, body.Length);
        lock (_cache) _cache.Add(entry);
    }

    // Allocation-heavy: string concatenation in a loop (classic StringBuilder miss).
    public void BuildReports(CancellationToken ct)
    {
        var rng = new Random(7);
        while (!ct.IsCancellationRequested)
        {
            var report = "";
            for (var i = 0; i < 200; i++)
                report += $"line {i}: {rng.Next()} {Guid.NewGuid()}\n";
            _ = Encoding.UTF8.GetByteCount(report);
        }
    }

    // Lock contention: everyone serializes on one lock and holds it while "calling the DB".
    public void ReserveInventory(CancellationToken ct)
    {
        var rng = new Random(Environment.CurrentManagedThreadId);
        while (!ct.IsCancellationRequested)
        {
            var sku = $"sku-{rng.Next(1000):D4}";
            lock (_inventoryLock)
            {
                _inventory[sku]--;
                Spin(TimeSpan.FromMilliseconds(0.05));
            }
        }
    }

    // Blocking I/O: synchronous, flushed small writes.
    public void WriteAuditLog(CancellationToken ct)
    {
        var path = Path.Combine(Path.GetTempPath(), $"dotnet-demo-audit-{Environment.ProcessId}.log");
        try
        {
            using var file = new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.Read, bufferSize: 1);
            var line = Encoding.UTF8.GetBytes(new string('x', 512) + "\n");
            while (!ct.IsCancellationRequested)
            {
                file.Write(line);
                file.Flush(flushToDisk: true);
            }
        }
        finally
        {
            File.Delete(path);
        }
    }

    private static void Spin(TimeSpan d)
    {
        var sw = Stopwatch.StartNew();
        while (sw.Elapsed < d) { }
    }

    public string Summary()
    {
        lock (_cache) return $"{Interlocked.Read(ref _handled):N0} requests, {_cache.Count:N0} cached entries";
    }
}
