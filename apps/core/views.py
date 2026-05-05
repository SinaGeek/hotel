from django.shortcuts import render
from django.contrib.auth.decorators import login_required
from apps.rooms.models import Room
from apps.guests.models import Guest
from apps.reservations.models import Reservation
from apps.inventory.models import InventoryItem

from apps.rooms.models import Room, Table, RoomType

def dashboard_view(request):
    user = request.user
    
    if request.method == 'POST' and user.is_superuser:
        # Tables management remains for now, but Rooms are handled in room_overview
        for cap in [2, 4, 6]:
            new_count = int(request.POST.get(f'table_count_{cap}', 0))
            current_count = Table.objects.filter(capacity=cap).count()
            if new_count > current_count:
                for i in range(new_count - current_count):
                    Table.objects.create(number=f"T{cap}{current_count+i+1}", capacity=cap)
            elif new_count < current_count:
                Table.objects.filter(capacity=cap).order_by('-id')[:current_count - new_count].delete()
        
        from django.shortcuts import redirect
        return redirect('dashboard')

    context = {
        'total_rooms': Room.objects.count(),
        'total_guests': Guest.objects.count(),
        'total_reservations': Reservation.objects.count(),
        'total_inventory': InventoryItem.objects.count(),
    }
    
    from django.utils.translation import get_language
    lang = get_language()
    
    rooms = Room.objects.all()
    room_types = RoomType.objects.all()
    
    room_stats = []
    for rt in room_types:
        room_stats.append({
            'name': rt.name,
            'total': rooms.filter(room_type=rt).count(),
            'occupied': rooms.filter(room_type=rt, status='OCCUPIED').count()
        })

    context['room_stats_list'] = room_stats
    context['room_clean_stats'] = {
        'clean': rooms.filter(is_clean=True).count(),
        'dirty': rooms.filter(is_clean=False).count(),
    }
    
    tables = Table.objects.all()
    context['restaurant_stats'] = {
        'tables_6': {'total': tables.filter(capacity=6).count(), 'occupied': tables.filter(capacity=6, status='occupied').count()},
        'tables_4': {'total': tables.filter(capacity=4).count(), 'occupied': tables.filter(capacity=4, status='occupied').count()},
        'tables_2': {'total': tables.filter(capacity=2).count(), 'occupied': tables.filter(capacity=2, status='occupied').count()},
        'total_guests': 0,
        'reserved_tables': tables.filter(status='reserved').count()
    }
    
    # Low stock alerts (max 3)
    context['low_stock'] = InventoryItem.objects.filter(quantity__lte=5)[:3]

    return render(request, 'dashboard.html', context)

from django.http import JsonResponse
import json

def set_theme(request):
    if request.method == 'POST':
        data = json.loads(request.body)
        theme = data.get('theme', 'light')
        request.session['theme'] = theme
        return JsonResponse({'status': 'success'})
    return JsonResponse({'status': 'failed'}, status=400)
