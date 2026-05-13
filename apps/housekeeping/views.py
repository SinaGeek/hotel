from django.shortcuts import render, get_object_or_404, redirect
from django.contrib.auth.decorators import login_required
from apps.rooms.models import Room
from apps.reservations.models import Reservation
from apps.inventory.models import InventoryItem, InventoryTransaction
from .models import LostAndFound
import datetime
from django.utils import timezone
from django.db import transaction

@login_required
def housekeeping_overview_view(request):
    today = datetime.date.today()
    rooms = Room.objects.all().order_by('floor', 'number')
    
    # Active reservations for today's status
    active_reservations = Reservation.objects.filter(
        check_in_date__lte=today, 
        check_out_date__gte=today
    ).select_related('guest')
    
    room_data = []
    for room in rooms:
        res = active_reservations.filter(room=room).first()
        is_occupied = (res.is_occupied if res else False)
        is_checkout_today = Reservation.objects.filter(room=room, check_out_date=today).exists()
        is_checkin_today = Reservation.objects.filter(room=room, check_in_date=today).exists()
        
        # One week dirty logic
        dirty_by_time = False
        if room.is_clean and room.last_cleaned:
            if (timezone.now() - room.last_cleaned).days >= 7:
                dirty_by_time = True
        
        room_data.append({
            'room': room,
            'is_occupied': is_occupied,
            'guest_name': f"{res.guest.first_name} {res.guest.last_name}" if res else "",
            'is_checkout_today': is_checkout_today,
            'is_checkin_today': is_checkin_today,
            'dirty_by_time': dirty_by_time,
        })
    
    return render(request, 'housekeeping/overview.html', {'rooms': room_data})

@login_required
def housekeeping_detail_view(request, room_id):
    room = get_object_or_404(Room, id=room_id)
    today = datetime.date.today()
    res = Reservation.objects.filter(room=room, check_in_date__lte=today, check_out_date__gte=today).first()
    consumables = InventoryItem.objects.filter(is_consumable=True)
    lost_items = LostAndFound.objects.filter(room=room).order_by('-id')

    if request.method == 'POST':
        action = request.POST.get('action')
        
        if action == 'update_status':
            room.needs_maintenance = request.POST.get('needs_maintenance') == 'on'
            room.save()
            
        elif action == 'mark_cleaned':
            room.is_clean = True
            room.status = 'CLEAN'
            room.last_cleaned = timezone.now()
            room.save()
            
        elif action == 'restock':
            item_ids = request.POST.getlist('item_ids')
            with transaction.atomic():
                for item_id in item_ids:
                    qty_str = request.POST.get(f'qty_{item_id}')
                    if not qty_str: continue
                    qty = float(qty_str)
                    if qty <= 0: continue
                    
                    item = get_object_or_404(InventoryItem, id=item_id)
                    item.quantity -= qty
                    item.save()
                    InventoryTransaction.objects.create(
                        item=item,
                        transaction_type='ISSUE',
                        quantity=qty,
                        user=request.user,
                        notes=f"Restock Room #{room.number}"
                    )
                room.last_restocked = timezone.now()
                room.needs_inventory = False
                room.save()
                
        elif action == 'add_lost_found':
            item_name = request.POST.get('item_name')
            if item_name:
                LostAndFound.objects.create(
                    room=room,
                    item_name=item_name,
                    description=request.POST.get('description', ''),
                    photo=request.FILES.get('photo'),
                    found_by=request.user
                )
        
        return redirect('housekeeping_detail', room_id=room.id)

    return render(request, 'housekeeping/detail.html', {
        'room': room,
        'reservation': res,
        'consumables': consumables,
        'lost_items': lost_items
    })

@login_required
def update_room_status(request, room_id):
    # This view is now mostly a dispatcher for the detail page actions if needed
    return redirect('housekeeping_detail', room_id=room_id)
