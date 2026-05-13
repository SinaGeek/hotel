from django.shortcuts import render
from django.contrib.auth.decorators import login_required
from apps.rooms.models import Room
from apps.guests.models import Guest
from apps.reservations.models import Reservation, Transaction
from apps.inventory.models import InventoryItem
import datetime
from django.db.models import Sum
from django.contrib.auth import get_user_model
User = get_user_model()

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
        rt_rooms = rooms.filter(room_type=rt)
        vacant = rt_rooms.filter(status__in=['CLEAN', 'DIRTY', 'VACANT']).count()
        reserved = rt_rooms.filter(status='RESERVED').count()
        total = rt_rooms.count()
        room_stats.append({
            'id': rt.id,
            'name': rt.name,
            'total': total,
            'vacant': vacant,
            'reserved': reserved,
            'display': f"{vacant} / {reserved} / {total}"
        })

    # Financial Stats
    today = datetime.date.today()
    today_revenue = Transaction.objects.filter(date=today).aggregate(total=Sum('amount_ref'))['total'] or 0
    
    # Simple pending calculation
    pending_data = Reservation.objects.filter(is_paid=False).aggregate(
        total=Sum('total_amount'), 
        paid=Sum('paid_amount')
    )
    total_pending = (pending_data['total'] or 0) - (pending_data['paid'] or 0)

    context['financials'] = {
        'today_revenue': today_revenue,
        'total_pending': total_pending,
    }
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
    
    if user.is_authenticated and user.is_superuser:
        context['pending_users'] = User.objects.filter(is_active=False)

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
