# Web supports and spatial silk

Web-building spiders use habitat walls and furniture as supports. Outside a
habitat they use visible window frames and accessible UI outlines in the front
windows. Windows UI Automation samples controls, links, text blocks, and
images without requesting names, text, passwords, or values. Runtime IDs keep
attachments associated with their original elements. Moving, scrolling, resizing,
closing, covering, or removing those supports disrupts their silk.

UI sampling runs on one background COM worker, with a bounded request queue,
750 ms sampling interval rotating across the three front windows, traversal
limits, and a three-second expiry for stale results. The animation thread never
waits on a provider. Applications that do
not expose accessibility geometry retain the window-frame fallback. Content
changes that preserve an element's identity and outline are not a broken support.

Silk nodes have persistent 3D positions and velocities. A damped, tension-only
constraint solver pins supported ends, allows junctions to sag, and lets released
silk fall. Splitting a thread interpolates its physical position, preserving the
attachment. The renderer draws curved strands and places the spider and caught
prey on the physical silk. Orb webs stand upright, house-spider tangles occupy
depth under the lid, and funnel sheets spread near the substrate. Gumfoot pins
sit beneath their upper junctions. Furniture attachment surfaces follow the
vivarium bark, twig, and pebble dimensions.

The species construction programs and prey/vibration queries retain their 2D
construction chart. This is a biological approximation, not calibrated silk
material physics or general 3D collision/navigation. UI sensing relies on the
application's accessibility provider; arbitrary canvas pixels are not segmented.
These changes apply to the three web-building species; jumping-spider retreats
and safety lines retain their existing behavior.

Validation covers pinned endpoints, sag, released silk falling, thread splitting,
support loss, stale UI data, furnished construction with physics, species spatial
layout, and a bounded live accessibility smoke test. Build with `fly.bat`; an
already-running copy must be restarted to use the new executable.
