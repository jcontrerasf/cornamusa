<script lang="ts">
  import type { EmailAddress } from '../api';
  import { colorFor, initials } from '../util';

  let { address, size = 36 }: { address: EmailAddress | null | undefined; size?: number } = $props();
  const bg = $derived(colorFor(address?.email ?? '?'));
</script>

<div class="avatar" style:width="{size}px" style:height="{size}px" style:font-size="{size * 0.38}px" style:background={bg}>
  {initials(address)}
</div>

<style>
  .avatar {
    border-radius: 50%;
    color: #fff;
    display: grid;
    place-items: center;
    font-weight: 600;
    flex-shrink: 0;
    letter-spacing: 0.02em;
  }
</style>
