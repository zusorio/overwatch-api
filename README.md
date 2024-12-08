> [!WARNING]  
> Unfortunately, Fly have significantly increased their prices which means I can no longer host this API.

# Very Cool Overwatch API by Zusor

Blizzard doesn't have an official API for getting Overwatch player data. They do have [a website](https://playoverwatch.com/en-us/search/) which shows some limited data but it's not easily machine-readable.

So I built my own.

To use it, make a `GET` request to [`/v1/player/Zusor-2553`](https://overwatch-api.zusor.io/v1/player/Zusor-2553).

To learn more, check out the [interactive docs](https://overwatch-api.zusor.io/docs).

There are some other unofficial APIs which inspired this project ([ow-api](https://ow-api.com/) and the now-defunct [ovrstat](https://github.com/s32x/ovrstat)), but they're missing some new Overwatch 2 features and include a lot of data that I don't need.

I also used this project as a way to learn Rust and tried to focus on making it as fast as possible. Workers are distributed globally (using [fly.io](https://fly.io)) to minimize latency and player data is cached for 10 minutes.

The code is MIT-licensed and available at [GitHub](https://github.com/zusorio/overwatch-api).
