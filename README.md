# Etclient

A light client for Ethereum Classic built on top of SputnikVM and `etcommon-worldstate` storage.

Light in the storage sense as Parity's wrap mode and Geth's fast sync. In the network sense, it will also support disabling P2P, i.e. use a different downloader (with reduced security) in P2P-not-welcomed network conditions.

Note that there's no code there yet, and there's no plan for providing anything other than a light client -- no full client, no `web3`/FFI/JSON-RPC/wallet (those should be provided by another library), no GUI (use Emerald instead).
